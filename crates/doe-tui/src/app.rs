//! The state of the viewer and what a key does to it. Nothing here touches a
//! terminal, so it is tested by calling it.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use doe::DoeError;
use doe::filesystem::Algorithm;

use crate::Options;
use crate::builder::Builder;
use crate::bytes::FileBytes;
use crate::draft::{Container, Count, Draft, DraftError, Edit, Field, Kind as DraftKind};
use crate::field_dialog;
use crate::files_view::FilesState;
use crate::format;
use crate::hex_view::{HexState, Selection};
use crate::keymap::{self, Action, Scope};
use crate::load::{self, LoadFailure, Loaded};
use crate::preview::{Outcome as PreviewOutcome, PREVIEW_BYTES, Preview};
use crate::prompt::{Kind, LineEditor, Outcome as PromptOutcome};
use crate::radixes::Radixes;
use crate::report::Report;
use crate::schemas::{Outcome as SchemasOutcome, SchemaList};
use crate::search::{Needle, Outcome as SearchOutcome, Search};
use crate::theme_browser::ThemeBrowser;
use crate::tree::{Shown, TreeModel};
use crate::tree_view::TreeView;
use crate::types::{Outcome as TypesOutcome, TypeBrowser};

/// How far from the start of a field of text that ends with a byte the byte is looked for.
const TERMINATOR_SEARCH: usize = 64 * 1024;

/// The most bytes that `y` copies.
pub const COPY_BYTES: usize = 4096;
/// The most that `Y` copies, in bytes of the field or of the text.
pub const COPY_LIMIT: usize = 64 * 1024;
/// What `Y` can copy: the letter and what it makes, in the order of the chooser.
pub const COPY_FORMATS: [(char, &str); 8] = [
    ('p', "path"),
    ('v', "value"),
    ('j', "JSON"),
    ('x', "hex"),
    ('X', "hex, no spaces"),
    ('c', "C array"),
    ('o', "offset+length"),
    ('s', "schema field"),
];

/// The type of the field as the schema writes it: `u2`, `s4`, `f8`, `b3`, `str`, `bytes`, or the name of the type.
fn schema_kind(row: &crate::tree::Row) -> String {
    use crate::tree::Shown;
    let bits = row.span.map_or(0, |s| s.end.saturating_sub(s.start));
    let sized = |letter: char| {
        if bits.is_multiple_of(8) && bits > 0 {
            format!("{letter}{}", bits / 8)
        } else {
            format!("b{bits}")
        }
    };
    match (&row.shown, &row.type_name) {
        (_, Some(name)) => name.rsplit("::").next().unwrap_or(name).to_owned(),
        (Some(Shown::UInt(_) | Shown::Enum { .. }), _) => sized('u'),
        (Some(Shown::SInt(_)), _) => sized('s'),
        (Some(Shown::Float(_)), _) => sized('f'),
        (Some(Shown::Bool(_)), _) => "b1".to_owned(),
        (Some(Shown::Str { .. }), _) => "str".to_owned(),
        (Some(Shown::Bytes { .. }), _) => "bytes".to_owned(),
        (None, None) => "type".to_owned(),
    }
}

/// How many fields a search looks at between two redraws of the screen.
pub const SEARCH_STEP: usize = 5_000;

/// The pane that keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The tree of fields.
    Tree,
    /// The bytes.
    Hex,
}

/// What the questions about a field lead to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogAction {
    /// A field is added.
    Add,
    /// The field at this place is renamed.
    Rename(usize),
    /// The field at this place gets another kind.
    Retype(usize),
}

/// What is known of a field that is being added, between one question and the next.
#[derive(Debug, Clone)]
struct FieldDialog {
    /// What the questions are for.
    action: DialogAction,
    /// Where the field starts, in bytes.
    at: u64,
    /// How many bytes to skip before it, from where the schema ends.
    gap: u64,
    /// The length of the selection that starts at `at`, if there is one.
    selection: Option<u64>,
    /// The selection as hex, to propose as a signature.
    selection_hex: String,
    /// The kind, once asked.
    kind: String,
    /// The length of bytes or text, once asked.
    length: Option<u64>,
    /// The encoding of text, once asked.
    encoding: String,
    /// The bytes of a signature, once asked.
    magic: Vec<u8>,
    /// The byte that ends text, once asked.
    terminator: u8,
    /// How often the field is repeated, once asked.
    repeat: Option<Count>,
    /// The id of the type of the field, when its kind is a type of the type paths.
    type_id: String,
    /// The size of the window that such a field is read from, once asked.
    size: Option<u64>,
    /// Where the field is added, or the field that is renamed or changed is: the schema or a type of it.
    container: Container,
    /// The row of the tree that the field (or the field that has the type it is added into) is.
    path: Vec<usize>,
    /// The offset in the file where an instance of that type starts, to measure from.
    type_start: u64,
    /// The field is of a new type that is empty (`new`): the type is made with the field.
    new_type: bool,
}

impl FieldDialog {
    fn new(action: DialogAction, at: u64) -> FieldDialog {
        FieldDialog {
            action,
            at,
            gap: 0,
            selection: None,
            selection_hex: String::new(),
            kind: String::new(),
            length: None,
            encoding: field_dialog::DEFAULT_ENCODING.to_owned(),
            magic: Vec::new(),
            terminator: 0,
            repeat: None,
            type_id: String::new(),
            size: None,
            container: Container::Top,
            path: Vec::new(),
            type_start: 0,
            new_type: false,
        }
    }
}

/// The fields that hold one byte, and the one that is selected.
#[derive(Debug, Clone)]
pub(crate) struct Overlap {
    /// The byte.
    offset: u64,
    /// The paths of the fields, the one in the sequence first.
    paths: Vec<Vec<usize>>,
    /// The one that is selected.
    at: usize,
}

/// The line that is being typed at the bottom of the screen.
#[derive(Debug, Clone)]
pub struct Prompt {
    /// What it is for.
    pub kind: Kind,
    /// The text and the cursor.
    pub line: LineEditor,
}

/// The viewer.
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate thing that is shown or not; they are not a state machine"
)]
pub struct App {
    /// The file that is shown.
    pub file: PathBuf,
    /// The type it was parsed as.
    pub type_name: String,
    /// Its size in bytes.
    pub size: u64,
    /// Why the parse stopped, if it did.
    pub error: Option<DoeError>,
    /// The tree of what was read.
    pub tree: TreeModel,
    /// The entries as directories and files, when the schema says the file is a file system.
    pub files: Option<FilesState>,
    /// Whether the files are shown in the place of the tree of fields (the default when there are some).
    pub files_wanted: bool,
    /// The node of the tree that the files were last made to follow.
    files_followed: Vec<usize>,
    /// The preview of a file of the archive, while it is open.
    pub preview: Option<Preview>,
    /// Which kind of value owns each byte, for the colours of the bytes pane; made when the bytes are first drawn.
    pub byte_kinds: Option<crate::kinds::ByteKinds>,
    /// Whether the bytes are coloured by the fields they belong to (`B`).
    pub byte_colours: bool,
    /// Whether the Markdown entries that are previewed are shown rendered (kept while the viewer is open).
    pub markdown_rendered: bool,
    /// Where the tree pane is scrolled to.
    pub view: TreeView,
    /// The bytes of the file, read as they are shown (none if it cannot be opened now).
    pub bytes: Option<FileBytes>,
    /// Where the hex pane is.
    pub hex: HexState,
    /// Which pane has the keys.
    pub focus: Focus,
    /// Whether the inspector shows the selected bytes in other forms.
    pub inspector: bool,
    /// Whether the file is still being parsed (in another thread): the screen
    /// is up and says so, and the tree comes when it is done.
    pub loading: bool,
    /// The temporary directory that holds the layer being shown (a `.tar.gz` shown as its tar), removed when it is replaced.
    layer_guard: Option<tempfile::TempDir>,
    /// Which layer of the file is shown (0 is the file itself), once known.
    layer_index: Option<usize>,
    /// Whether the focused pane fills the screen (`f`); the pane that has the focus is the one that is shown.
    pub fullscreen: bool,
    /// When the parse began, for the time it has taken.
    pub loading_since: Option<std::time::Instant>,
    /// Whether the help is shown.
    pub help: bool,
    /// How far the help is scrolled, and its search.
    pub help_view: crate::help::HelpView,
    /// What went wrong, if something did: a parse that stopped, or nothing parsed at all.
    pub report: Option<Report>,
    /// Whether the message about a parse that failed is shown.
    pub show_report: bool,
    /// The line being typed, if any.
    pub prompt: Option<Prompt>,
    /// The search that is going on, if any (it is advanced by [`App::search_step`]).
    pub search: Option<Search>,
    /// What the viewer was started with, to read it all again (`r`).
    pub options: Option<Options>,
    /// The schema files that were read, and the file: what `--watch` looks at.
    pub schema_files: Vec<std::path::PathBuf>,
    /// The list of the schemas and their types, while it is open.
    pub schemas: Option<SchemaList>,
    /// The browser of the types on the type paths, while it is open.
    pub types: Option<TypeBrowser>,
    /// The list of the colour themes, while it is open.
    pub themes: Option<ThemeBrowser>,
    /// The theme chosen in the viewer (or being previewed), which the screen is
    /// drawn with instead of the one it was started with; none: that one.
    pub styles: Option<crate::ui::Theme>,
    /// The id of the theme in use (`terminal` for the terminal's own), none: as started.
    pub theme_id: Option<String>,
    /// How many colours the terminal shows, for the themes.
    pub depth: crate::theme::Depth,
    /// The field that `d` asked to be removed again for, until the next key.
    remove_armed: Option<usize>,
    /// The same, for the key that is being handled.
    remove_was_armed: Option<usize>,
    /// The questions of a field that is being added, while they are asked.
    field_dialog: Option<FieldDialog>,
    /// The questions of a field whose kind is being chosen from the types of the type paths (the browser of types is open).
    field_pick: Option<FieldDialog>,
    /// The fields that are being made a type, while its name is asked: where, from which, how many.
    group_plan: Option<(Container, usize, usize, Vec<usize>)>,
    /// The field that is being added, while the name of the new empty type it has is asked.
    new_type_dialog: Option<FieldDialog>,
    /// Hex that `y` asked to be put on the clipboard, until the event loop does it.
    clipboard: Option<String>,
    /// Whether the chooser of what to copy (`Y`) is open and waits for a letter.
    pub copy_as: bool,
    /// The places jumped from, to go back to.
    jumps: crate::history::History,
    /// The schema that is being built, when the viewer was started with `--build`.
    pub builder: Option<Builder>,
    /// Whether `q` was pressed on a schema that is not saved, so that a second one leaves.
    quit_armed: bool,
    /// Whether the key being handled came right after that.
    quit_was_armed: bool,
    /// The fields that hold the byte under the cursor, while they are being cycled through (`.`).
    pub(crate) overlap: Option<Overlap>,
    /// The radix of the integers: one for all, and some of the fields.
    pub radixes: Radixes,
    /// What was last searched for, for `n` and `N`.
    pub last_query: Option<String>,
    /// A message for the status bar, until the next key.
    pub notice: Option<String>,
    /// Whether the person asked to leave.
    pub quit: bool,
}

/// An offset as it is typed: decimal or `0x` hex, `_` allowed between digits.
///
/// # Errors
///
/// What is wrong with the text.
pub fn parse_offset(text: &str) -> Result<u64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("type an offset: a number, or 0x… for hex".to_owned());
    }
    let (digits, radix) = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => (hex, 16),
        None => (text, 10),
    };
    let cleaned: String = digits.chars().filter(|&c| c != '_').collect();
    if cleaned.is_empty() || cleaned.starts_with('-') || cleaned.starts_with('+') {
        return Err(format!("not an offset: {}", crate::format::safe(text)));
    }
    u64::from_str_radix(&cleaned, radix).map_err(|e| {
        if matches!(e.kind(), std::num::IntErrorKind::PosOverflow) {
            format!("{} is too large for an offset", crate::format::safe(text))
        } else {
            format!("not an offset: {}", crate::format::safe(text))
        }
    })
}

/// The most bytes of a file that are read to unwrap it (what `process:` allows to be produced).
const UNWRAP_LIMIT: usize = 256 * 1024 * 1024;

/// Open the type `id` on the file as the layers say: a type that holds another file (a `.tar.gz`'s `gzip_file`) is shown as the
/// layer it holds, from a temporary file. `options` are changed to say so; the directory is returned, and a line for the
/// notice. A type that holds nothing leaves them as they are.
fn unwrap_for(options: &mut Options, id: &str) -> (Option<tempfile::TempDir>, Option<String>) {
    use std::io::Write as _;

    let outer = options
        .outer_file
        .clone()
        .unwrap_or_else(|| options.file.clone());
    let Ok(found) = doe::layers::unwrap_as(
        &outer,
        &options.include_paths,
        &doe::detect::Options::default(),
        doe::layers::MAX_DEPTH,
        UNWRAP_LIMIT,
        Some(id),
    ) else {
        return (None, None);
    };
    if found.layers.len() < 2 {
        // it holds something that cannot be shown (or nothing): the file as it was opened, if a layer was shown
        if let Some(note) = found.notes.first() {
            options.file = outer;
            options.outer_file = None;
            return (None, Some(note.clone()));
        }
        return (None, None);
    }
    let (Some(inner), Some(data)) = (found.layers.last(), found.data.last()) else {
        return (None, None);
    };
    let mut name = outer
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("layer")
        .to_owned();
    for _ in 1..found.layers.len() {
        name = doe::layers::payload_name(&name);
    }
    let Ok(dir) = tempfile::Builder::new().prefix("doe-layer-").tempdir() else {
        return (None, None);
    };
    let path = dir
        .path()
        .join(if name.is_empty() { "layer" } else { &name });
    if std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(data))
        .is_err()
    {
        return (None, None);
    }
    options.file = path;
    options.outer_file = Some(outer);
    options.type_or_schema.clone_from(&inner.type_name);
    (
        Some(dir),
        Some(format!("{}: showing the last layer", found.describe())),
    )
}

impl App {
    /// A viewer for a parsed file. If the parse stopped, the field where it did
    /// is selected and its bytes are shown.
    #[must_use]
    pub fn new(file: PathBuf, loaded: Loaded) -> Self {
        let mut app = App {
            bytes: FileBytes::open(&file).ok(),
            hex: HexState::new(),
            focus: Focus::Tree,
            inspector: false,
            loading: false,
            layer_guard: None,
            layer_index: None,
            fullscreen: false,
            loading_since: None,
            help: false,
            help_view: crate::help::HelpView::default(),
            report: None,
            show_report: true,
            options: None,
            schema_files: loaded.schema_files,
            schemas: None,
            types: None,
            themes: None,
            styles: None,
            theme_id: None,
            depth: crate::theme::Depth::True,
            builder: None,
            remove_armed: None,
            remove_was_armed: None,
            field_dialog: None,
            field_pick: None,
            group_plan: None,
            new_type_dialog: None,
            clipboard: None,
            copy_as: false,
            jumps: crate::history::History::default(),
            quit_armed: false,
            quit_was_armed: false,
            overlap: None,
            radixes: Radixes::default(),
            prompt: None,
            search: None,
            last_query: None,
            notice: None,
            file,
            type_name: loaded.type_name,
            size: loaded.size,
            error: loaded.error,
            files: loaded
                .filesystem
                .filter(|listing| !listing.entries.is_empty())
                .map(|listing| FilesState::new(&listing)),
            files_wanted: true,
            files_followed: Vec::new(),
            preview: None,
            byte_kinds: Some(loaded.byte_kinds),
            byte_colours: true,
            markdown_rendered: false,
            tree: TreeModel::new(loaded.trace, loaded.value).with_docs(loaded.docs),
            view: TreeView::new(),
            quit: false,
        };
        if let Some(error) = &app.error {
            let path = app.tree.failed_path().unwrap_or_default();
            let names = app.tree.names_to(&path);
            // the type the field is in: the nearest one above it that has a name
            let in_type = (0..path.len()).rev().find_map(|depth| {
                app.tree
                    .node(path.get(..depth)?)
                    .and_then(|n| n.type_name.clone())
            });
            app.report = Some(Report::from_parse(error, &names, in_type.as_deref()));
            app.tree.select_path(&path);
            app.show_selected_field();
        }
        app
    }

    /// A viewer for a file that could not be parsed at all: it shows what went
    /// wrong and where, and the bytes of the file if it can be read.
    #[must_use]
    pub fn broken(
        file: PathBuf,
        failure: &LoadFailure,
        schema_file: Option<&std::path::Path>,
    ) -> Self {
        App {
            bytes: FileBytes::open(&file).ok(),
            hex: HexState::new(),
            focus: Focus::Tree,
            inspector: false,
            loading: false,
            layer_guard: None,
            layer_index: None,
            fullscreen: false,
            loading_since: None,
            help: false,
            help_view: crate::help::HelpView::default(),
            report: Some(Report::from_failure(failure, schema_file)),
            show_report: true,
            options: None,
            schema_files: failure.schema_files.clone(),
            schemas: None,
            types: None,
            themes: None,
            styles: None,
            theme_id: None,
            depth: crate::theme::Depth::True,
            builder: None,
            remove_armed: None,
            remove_was_armed: None,
            field_dialog: None,
            field_pick: None,
            group_plan: None,
            new_type_dialog: None,
            clipboard: None,
            copy_as: false,
            jumps: crate::history::History::default(),
            quit_armed: false,
            quit_was_armed: false,
            overlap: None,
            radixes: Radixes::default(),
            prompt: None,
            search: None,
            last_query: None,
            notice: None,
            file,
            type_name: failure.type_name.clone(),
            size: failure.size,
            error: None,
            tree: TreeModel::new(
                doe::parser::trace::TraceNode::named(&failure.type_name),
                None,
            ),
            files: None,
            files_wanted: true,
            files_followed: Vec::new(),
            preview: None,
            byte_kinds: None,
            byte_colours: true,
            markdown_rendered: false,
            view: TreeView::new(),
            quit: false,
        }
    }

    /// A viewer for a file that is being parsed: there is nothing to show yet but
    /// that it is being read. [`App::finish_loading`] gives it the result.
    #[must_use]
    pub fn loading(file: PathBuf, type_name: &str) -> Self {
        let mut app = App {
            bytes: FileBytes::open(&file).ok(),
            hex: HexState::new(),
            focus: Focus::Tree,
            inspector: false,
            loading: true,
            layer_guard: None,
            layer_index: None,
            fullscreen: false,
            loading_since: Some(std::time::Instant::now()),
            help: false,
            help_view: crate::help::HelpView::default(),
            report: None,
            show_report: true,
            options: None,
            schema_files: Vec::new(),
            schemas: None,
            types: None,
            themes: None,
            styles: None,
            theme_id: None,
            depth: crate::theme::Depth::True,
            builder: None,
            remove_armed: None,
            remove_was_armed: None,
            field_dialog: None,
            field_pick: None,
            group_plan: None,
            new_type_dialog: None,
            clipboard: None,
            copy_as: false,
            jumps: crate::history::History::default(),
            quit_armed: false,
            quit_was_armed: false,
            overlap: None,
            radixes: Radixes::default(),
            prompt: None,
            search: None,
            last_query: None,
            notice: None,
            size: 0,
            type_name: type_name.to_owned(),
            error: None,
            tree: TreeModel::new(doe::parser::trace::TraceNode::named(type_name), None),
            files: None,
            files_wanted: true,
            files_followed: Vec::new(),
            preview: None,
            byte_kinds: None,
            byte_colours: true,
            markdown_rendered: false,
            view: TreeView::new(),
            quit: false,
            file,
        };
        app.size = app.bytes.as_ref().map_or(0, FileBytes::size);
        app
    }

    /// The parse that was going on is done: show what it made, or what stopped it.
    pub fn finish_loading(&mut self, result: Result<Loaded, Box<LoadFailure>>) {
        match result {
            Ok(loaded) => {
                if let Some(builder) = self.builder.as_mut() {
                    // a schema that was gone on with: the file decides some lengths
                    builder.draft.fit_to_trace(&loaded.trace);
                }
                self.adopt(loaded);
            }
            Err(failure) => {
                let schema = self.options.as_ref().and_then(load::schema_file);
                let broken = App::broken(self.file.clone(), &failure, schema.as_deref());
                self.loading = false;
                self.loading_since = None;
                self.tree = broken.tree;
                self.byte_kinds = None;
                self.type_name = broken.type_name;
                self.size = broken.size;
                self.error = None;
                self.report = broken.report;
                self.bytes = broken.bytes;
                self.schema_files = broken.schema_files;
            }
        }
        self.notice = None;
    }

    /// The files whose change means a reload: the schema and what it imports, and the file.
    #[must_use]
    pub fn watched_files(&self) -> Vec<PathBuf> {
        let mut files = self.schema_files.clone();
        if !files.contains(&self.file) {
            files.push(self.file.clone());
        }
        files
    }

    /// Read the schema (with its imports) and the file again and parse.
    ///
    /// The tree is the new one, with the same fields open and the selection on
    /// the field with the same name (or the nearest one that is there), and the
    /// scroll where it was as far as the new tree allows. If the new schema or
    /// file cannot be used, the old tree stays and the error is shown.
    pub fn reload(&mut self) {
        if self.loading {
            self.notice = Some("still reading the file".to_owned());
            return;
        }
        let Some(options) = self.options.clone() else {
            self.notice = Some("nothing to read again from".to_owned());
            return;
        };
        match load::try_load(&options) {
            Ok(loaded) => self.adopt(loaded),
            Err(failure) => self.reload_failed(&failure, &options),
        }
    }

    /// The next radix for the selected field, which holds for every element and
    /// after the schema is read again.
    fn cycle_field_radix(&mut self) {
        let row = self.tree.selected();
        let integer = matches!(
            row.shown,
            Some(Shown::UInt(_) | Shown::SInt(_) | Shown::Enum { .. })
        );
        let Some(key) = self.tree.field_key(&row.path).filter(|_| integer) else {
            self.notice = Some("the selected field is not an integer".to_owned());
            return;
        };
        self.notice = Some(match self.radixes.cycle_field(&key) {
            Some(radix) => format!("this field in {}", radix.name()),
            None => format!(
                "this field follows the others ({})",
                self.radixes.global.name()
            ),
        });
    }

    /// Take a new parse in the place of the old, keeping the place.
    fn adopt(&mut self, loaded: Loaded) {
        let was_broken = self.report.as_ref().is_some_and(|r| r.fatal) || self.loading;
        self.loading = false;
        self.loading_since = None;
        let fresh = App::new(self.file.clone(), loaded);
        let mut tree = fresh.tree;
        if !was_broken {
            tree.take_state_of(&self.tree);
        }
        // the descriptions stay shown or hidden as they were
        tree.set_docs_shown(self.tree.docs_shown());
        self.tree = tree;
        self.byte_kinds = fresh.byte_kinds;
        let algorithm = self.files.as_ref().and_then(|f| f.algorithm);
        self.files = fresh.files;
        // the column stays on across a reload, with the checksums made again from the file as it is now
        if let (Some(algorithm), Some(files)) = (algorithm, self.files.as_mut()) {
            files.algorithm = Some(algorithm);
            let ranges = files.checksum_ranges();
            files.checksums.start(&self.file, algorithm, &ranges);
        }
        self.files_followed = Vec::new();
        // the entries may be somewhere else now
        self.preview = None;
        self.type_name = fresh.type_name;
        self.size = fresh.size;
        self.error = fresh.error;
        self.report = fresh.report;
        self.bytes = fresh.bytes;
        self.schema_files = fresh.schema_files;
        self.search = None;
        self.hex.clear_range();
        if was_broken {
            self.hex = fresh.hex;
        } else {
            self.show_selected_field();
        }
        self.notice = Some("read again".to_owned());
    }

    /// The new schema or file could not be used: the old tree stays.
    fn reload_failed(&mut self, failure: &LoadFailure, options: &Options) {
        let was_broken = self.report.as_ref().is_some_and(|r| r.fatal);
        let mut report = Report::from_failure(failure, load::schema_file(options).as_deref());
        report.fatal = was_broken;
        report.headline = format!("could not read again: {}", report.headline);
        self.notice = Some(report.one_line());
        self.report = Some(report);
        self.show_report = true;
        self.schema_files.clone_from(&failure.schema_files);
    }

    /// What is said after the viewer has been left, if the last parse failed:
    /// the message, and that the command exits with a failure.
    #[must_use]
    pub fn finish_message(&self) -> Option<String> {
        let report = self.report.as_ref()?;
        Some(format!(
            "{}: {}",
            crate::format::safe(&self.file.display().to_string()),
            report.one_line()
        ))
    }

    /// Whether a search is going on (the event loop then advances it between keys).
    #[must_use]
    pub fn searching(&self) -> bool {
        self.search.is_some()
    }

    /// Whether the viewer has work to do between keys: a search that goes on, or
    /// the schemas of the type browser that have not been read for their signatures.
    #[must_use]
    pub fn working(&self) -> bool {
        self.searching() || self.types.as_ref().is_some_and(|t| t.pending() > 0)
    }

    /// Do a piece of the work that goes on between keys.
    pub fn work_step(&mut self) {
        if self.searching() {
            self.search_step();
        } else {
            self.types_step();
        }
    }

    /// Read the schemas of some more types of the browser, for their signatures.
    fn types_step(&mut self) {
        let Some(include_paths) = self.options.as_ref().map(|o| o.include_paths.clone()) else {
            return;
        };
        if let Some(browser) = self.types.as_mut() {
            browser.step(&include_paths, std::time::Duration::from_millis(20));
        }
    }

    /// What a key does. The keys are in [`keymap::BINDINGS`] and in the help
    /// (`?`); this decides who gets a key:
    ///
    /// - a search that is going on is stopped by any key (`Esc` and Ctrl-C
    ///   only stop it);
    /// - the help is scrolled and searched by its keys (`/`, `n`, `N`, the
    ///   arrows) and closed by any other key;
    /// - a prompt takes the key as text or as an edit of it;
    /// - otherwise the pane that has the keys, then the keys for both.
    pub fn on_key(&mut self, key: KeyEvent) {
        self.notice = None;
        self.quit_was_armed = std::mem::take(&mut self.quit_armed);
        self.remove_was_armed = self.remove_armed.take();
        if self.search.take().is_some() {
            let stops = key.code == KeyCode::Esc
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL));
            self.notice = Some("search stopped".to_owned());
            if stops {
                return;
            }
        }
        if self.help {
            if self.help_view.key(&key) == crate::help::Outcome::Close {
                self.help = false;
                self.help_view.reset();
            }
            return;
        }
        if self.schemas.is_some() {
            self.schemas_key(&key);
            return;
        }
        if self.types.is_some() {
            self.types_key(&key);
            return;
        }
        if self.themes.is_some() {
            self.themes_key(&key);
            return;
        }
        if self.prompt.is_some() {
            self.prompt_key(&key);
            return;
        }
        if self.copy_as {
            self.copy_as = false;
            if let KeyCode::Char(letter) = key.code {
                self.copy_as_letter(letter);
            } else {
                self.notice = Some("nothing copied".to_owned());
            }
            return;
        }
        if self.preview.is_some() {
            self.preview_key(&key);
            return;
        }
        // Esc puts the two panes back before it leaves
        if self.fullscreen && key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
            self.fullscreen = false;
            self.notice = Some("both panes are shown".to_owned());
            return;
        }
        let pane = match self.focus {
            Focus::Tree => Scope::Tree,
            Focus::Hex => Scope::Bytes,
        };
        if let Some(action) = keymap::lookup(pane, &key) {
            self.act(action);
        }
    }

    /// Open the list of the schemas (read from disk now) with the selection on
    /// the type that is parsed.
    fn open_schemas(&mut self) {
        let Some(options) = self.options.as_ref() else {
            self.notice = Some("there are no schemas to list".to_owned());
            return;
        };
        match SchemaList::read(options) {
            Ok(mut list) => {
                list.select_type(&self.type_name);
                self.schemas = Some(list);
            }
            Err(message) => self.notice = Some(message),
        }
    }

    /// Open the browser of the types on the type paths (read from disk now)
    /// with the selection on the type that is parsed.
    fn open_types(&mut self) {
        let Some(options) = self.options.as_ref() else {
            self.notice = Some("there are no type paths to browse".to_owned());
            return;
        };
        if options.include_paths.is_empty() {
            self.notice = Some("there are no type paths: give one with -I".to_owned());
            return;
        }
        let mut browser = TypeBrowser::read(&options.include_paths);
        let (schema, _) = doe::split_type_arg(&options.type_or_schema);
        browser.select_type(schema);
        self.types = Some(browser);
    }

    /// Open the list of the colour themes, with the selection on the one in use.
    fn open_themes(&mut self) {
        let (dir, color) = self
            .options
            .as_ref()
            .map_or((None, true), |o| (o.theme_dir.clone(), o.color));
        self.themes = Some(ThemeBrowser::read(
            dir.as_deref(),
            self.styles,
            self.theme_id.clone(),
            color,
        ));
    }

    /// Show the theme of the selected entry on the viewer, when there is colour
    /// and the entry can be used.
    fn preview_theme(&mut self) {
        let Some(browser) = self.themes.as_ref() else {
            return;
        };
        if !browser.color {
            return;
        }
        let transparent = self.options.as_ref().and_then(|o| o.transparent);
        if let Some(styles) = browser
            .current()
            .and_then(|e| e.styles(self.depth, transparent))
        {
            self.styles = Some(styles);
        }
    }

    /// A key while the list of themes is open: move (the viewer shows each
    /// theme), filter, use the selected one, or go back to the one that was in use.
    fn themes_key(&mut self, key: &KeyEvent) {
        let Some(browser) = self.themes.as_mut() else {
            return;
        };
        let control_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        let mut close = control_c;
        let mut choose = false;
        if !control_c {
            if browser.editing {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => browser.editing = false,
                    KeyCode::Backspace => browser.backspace(),
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        browser.type_char(c);
                    }
                    KeyCode::Up => browser.move_by(-1),
                    KeyCode::Down => browser.move_by(1),
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => close = true,
                    KeyCode::Up | KeyCode::Char('k') => browser.move_by(-1),
                    KeyCode::Down | KeyCode::Char('j') => browser.move_by(1),
                    KeyCode::PageUp => browser.move_by(-10),
                    KeyCode::PageDown => browser.move_by(10),
                    KeyCode::Home => browser.jump(false),
                    KeyCode::End | KeyCode::Char('G') => browser.jump(true),
                    KeyCode::Char('/') => browser.editing = true,
                    KeyCode::Enter => choose = true,
                    _ => {}
                }
            }
        }
        if close {
            // back to the theme that was in use
            self.styles = browser.original;
            self.themes = None;
            return;
        }
        if choose {
            self.choose_theme();
        } else {
            self.preview_theme();
        }
    }

    /// `Enter` on a theme: use it, and keep it in the configuration file (or say how to, when there is none).
    fn choose_theme(&mut self) {
        let Some(browser) = self.themes.as_ref() else {
            return;
        };
        let Some(entry) = browser.current() else {
            return;
        };
        if let Some(error) = &entry.error {
            self.notice = Some(format!(
                "{} cannot be used: {}",
                crate::format::safe(&entry.id),
                crate::format::safe(error)
            ));
            return;
        }
        if !browser.color {
            self.notice = Some("there is no colour in this run (NO_COLOR or --color never), so no theme is applied".to_owned());
            self.themes = None;
            return;
        }
        // the choice is kept in the configuration file of this run: the terminal's own colours take the key out
        let id = entry.id.clone();
        let name = crate::format::safe(&entry.name);
        let keep = (entry.group != crate::theme_browser::Group::Terminal).then(|| id.clone());
        let how = entry.how_to_keep();
        let target = self.options.as_ref().and_then(|o| {
            o.config_file
                .clone()
                .map(|path| (path, o.theme_on_command_line))
        });
        let message = match target {
            Some((path, on_command_line)) => {
                let said = match crate::config_write::save_theme(&path, keep.as_deref()) {
                    Ok(crate::config_write::Saved::Written) => {
                        format!("theme {name} used and saved to {}", path.display())
                    }
                    Ok(crate::config_write::Saved::Unchanged) => {
                        format!("theme {name} used; {} already has it", path.display())
                    }
                    Err(error) => {
                        format!("theme {name} for this session; not saved: {error:#}")
                    }
                };
                if on_command_line {
                    format!("{said}; --theme on the command line wins until it is removed")
                } else {
                    said
                }
            }
            None => format!("theme {name} for this session; {how}"),
        };
        self.theme_id = Some(id);
        self.notice = Some(message);
        self.preview_theme();
        self.themes = None;
    }

    /// A key while the browser of types is open: move, filter, read a type,
    /// choose one, or close it.
    fn types_key(&mut self, key: &KeyEvent) {
        let Some(browser) = self.types.as_mut() else {
            return;
        };
        let reading = browser.pager.is_some();
        let control_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        let outcome = if control_c {
            TypesOutcome::Close
        } else if browser.editing && !reading {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    browser.editing = false;
                }
                KeyCode::Backspace => browser.backspace(),
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    browser.type_char(c);
                }
                KeyCode::Up => browser.move_by(-1),
                KeyCode::Down => browser.move_by(1),
                _ => {}
            }
            TypesOutcome::Nothing
        } else {
            let step = |down: bool| if down { 10 } else { -10 };
            match key.code {
                KeyCode::Esc | KeyCode::Left | KeyCode::Char('q' | 'h') => browser.back(),
                KeyCode::Up | KeyCode::Char('k') => {
                    if reading {
                        browser.scroll(-1);
                    } else {
                        browser.move_by(-1);
                    }
                    TypesOutcome::Nothing
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if reading {
                        browser.scroll(1);
                    } else {
                        browser.move_by(1);
                    }
                    TypesOutcome::Nothing
                }
                KeyCode::PageUp | KeyCode::PageDown => {
                    let delta = step(key.code == KeyCode::PageDown);
                    if reading {
                        browser.scroll(delta);
                    } else {
                        browser.move_by(delta);
                    }
                    TypesOutcome::Nothing
                }
                KeyCode::Home => {
                    if reading {
                        browser.scroll_to(false);
                    } else {
                        browser.jump(false);
                    }
                    TypesOutcome::Nothing
                }
                KeyCode::End | KeyCode::Char('G') => {
                    if reading {
                        browser.scroll_to(true);
                    } else {
                        browser.jump(true);
                    }
                    TypesOutcome::Nothing
                }
                KeyCode::Char('/') if !reading => {
                    browser.editing = true;
                    TypesOutcome::Nothing
                }
                KeyCode::Char('r') if !reading => {
                    let file = self.file.clone();
                    if let Err(message) = browser.toggle_rank(&file) {
                        self.notice = Some(message);
                    }
                    TypesOutcome::Nothing
                }
                KeyCode::Char('v' | 'l') | KeyCode::Right if !reading => {
                    browser.view();
                    TypesOutcome::Nothing
                }
                KeyCode::Enter if !reading => browser.choose(),
                _ => TypesOutcome::Nothing,
            }
        };
        match outcome {
            TypesOutcome::Nothing => {}
            TypesOutcome::Close => {
                self.types = None;
                if self.field_pick.is_some() {
                    self.picked_library_type(None);
                }
            }
            TypesOutcome::Choose(id) => {
                self.types = None;
                if self.field_pick.is_some() {
                    self.picked_library_type(Some(&id));
                } else {
                    self.choose_available(&id);
                }
            }
        }
    }

    /// Show the next layer of a file that holds another file (`.tar.gz`: the gzip file, then the tar), and the file itself
    /// after the last. The layers are found again from the file each time, as `doe FILE` finds them.
    pub fn cycle_layer(&mut self) {
        if self.loading {
            self.notice = Some("still reading the file".to_owned());
            return;
        }
        let Some(mut options) = self.options.clone() else {
            return;
        };
        let outer = options
            .outer_file
            .clone()
            .unwrap_or_else(|| options.file.clone());
        let found = match doe::layers::unwrap(
            &outer,
            &options.include_paths,
            &doe::detect::Options::default(),
            doe::layers::MAX_DEPTH,
            UNWRAP_LIMIT,
        ) {
            Ok(found) => found,
            Err(e) => {
                self.notice = Some(format!("could not look inside: {e}"));
                return;
            }
        };
        let count = found.layers.len();
        if count < 2 {
            self.notice = Some(
                found
                    .notes
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "this file holds no other file".to_owned()),
            );
            return;
        }
        // a viewer that was opened on a layer shows the last one; one opened on the file shows the first
        let here = self.layer_index.unwrap_or(if options.outer_file.is_some() {
            count - 1
        } else {
            0
        });
        let next = (here + 1) % count;
        let Some(layer) = found.layers.get(next) else {
            return;
        };
        let mut guard = None;
        if next == 0 {
            options.file.clone_from(&outer);
            options.outer_file = None;
        } else {
            let Some(data) = found.data.get(next - 1) else {
                return;
            };
            let mut name = outer
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("layer")
                .to_owned();
            for _ in 0..next {
                name = doe::layers::payload_name(&name);
            }
            let Ok(dir) = tempfile::Builder::new().prefix("doe-layer-").tempdir() else {
                return;
            };
            let path = dir
                .path()
                .join(if name.is_empty() { "layer" } else { &name });
            if std::fs::write(&path, data).is_err() {
                return;
            }
            options.file = path;
            options.outer_file = Some(outer);
            guard = Some(dir);
        }
        options.type_or_schema.clone_from(&layer.type_name);
        match load::try_load(&options) {
            Ok(loaded) => {
                self.file.clone_from(&options.file);
                self.options = Some(options);
                self.adopt(loaded);
                self.layer_guard = guard;
                self.layer_index = Some(next);
                self.notice = Some(format!(
                    "layer {} of {count}: {}",
                    next + 1,
                    found.describe()
                ));
            }
            Err(failure) => self.reload_failed(&failure, &options),
        }
    }

    /// Parse with a type of the type paths: the viewer reads the file again as
    /// `doe --tui FILE TYPE` would.
    pub fn choose_available(&mut self, id: &str) {
        if self.loading {
            self.notice = Some("still reading the file".to_owned());
            return;
        }
        let Some(mut options) = self.options.clone() else {
            return;
        };
        id.clone_into(&mut options.type_or_schema);
        // a type that holds another file (`gzip_file`) is shown as what it holds, as when it is found
        let (guard, said) = unwrap_for(&mut options, id);
        match load::try_load(&options) {
            Ok(loaded) => {
                // the file may be a layer now (adopt reads the bytes from `self.file`)
                self.file.clone_from(&options.file);
                self.options = Some(options);
                self.adopt(loaded);
                self.layer_guard = guard.or_else(|| self.layer_guard.take());
                self.layer_index = None;
                self.notice = Some(said.unwrap_or_else(|| format!("parsing as {id}")));
            }
            // the old type stays, with the old tree
            Err(failure) => self.reload_failed(&failure, &options),
        }
    }

    /// A key while the list of the schemas is open: move, read a schema, choose
    /// a type, read the files again, or close it.
    fn schemas_key(&mut self, key: &KeyEvent) {
        let Some(list) = self.schemas.as_mut() else {
            return;
        };
        let reading = list.pager.is_some();
        let outcome = match key.code {
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('q' | 'h') => list.back(),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                SchemasOutcome::Close
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if reading {
                    list.scroll(-1);
                } else {
                    list.move_by(-1);
                }
                SchemasOutcome::Nothing
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if reading {
                    list.scroll(1);
                } else {
                    list.move_by(1);
                }
                SchemasOutcome::Nothing
            }
            KeyCode::PageUp | KeyCode::PageDown => {
                let down = key.code == KeyCode::PageDown;
                let step = if down { 10 } else { -10 };
                if reading {
                    list.scroll(step);
                } else {
                    list.move_by(step);
                }
                SchemasOutcome::Nothing
            }
            KeyCode::Home => {
                if reading {
                    list.scroll_to(false);
                } else {
                    list.jump(false);
                }
                SchemasOutcome::Nothing
            }
            KeyCode::End | KeyCode::Char('G') => {
                if reading {
                    list.scroll_to(true);
                } else {
                    list.jump(true);
                }
                SchemasOutcome::Nothing
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') if !reading => list.enter(),
            KeyCode::Char('r') => {
                self.reload();
                if let Some(options) = self.options.as_ref()
                    && let Ok(mut fresh) = SchemaList::read(options)
                {
                    fresh.select_type(&self.type_name);
                    self.schemas = Some(fresh);
                }
                SchemasOutcome::Nothing
            }
            _ => SchemasOutcome::Nothing,
        };
        match outcome {
            SchemasOutcome::Nothing => {}
            SchemasOutcome::Close => self.schemas = None,
            SchemasOutcome::Choose(name) => {
                let main_id = self
                    .schemas
                    .as_ref()
                    .and_then(|l| l.main_id().map(str::to_owned));
                self.schemas = None;
                self.choose_type(&name, main_id.as_deref());
            }
        }
    }

    /// Change the schema that is being built, parse the file with it, and show
    /// the result. If the file cannot be parsed with the new schema (a field
    /// does not fit in it, the schema does not compile), the change is taken
    /// back, the old tree stays, and the reason is the error.
    ///
    /// # Errors
    ///
    /// The change was refused, by the draft or by the file, or the viewer is
    /// not building a schema.
    pub fn edit_draft(
        &mut self,
        change: impl FnOnce(&mut Draft) -> Result<Edit, DraftError>,
    ) -> Result<(), String> {
        let Some(builder) = self.builder.as_mut() else {
            return Err("the viewer is not building a schema".to_owned());
        };
        let edit = change(&mut builder.draft).map_err(|e| e.to_string())?;
        match self.parse_draft() {
            Ok(()) => {
                if let Some(builder) = self.builder.as_mut() {
                    builder.history.push(edit);
                    builder.redo.clear();
                }
                Ok(())
            }
            Err(reason) => {
                if let Some(builder) = self.builder.as_mut() {
                    // the draft was valid before: taking the change back cannot fail
                    let _ = builder.draft.apply(&edit.inverse());
                    let _ = builder.write();
                }
                Err(reason)
            }
        }
    }

    /// Write the draft and parse the file with it; nothing changes if that fails.
    fn parse_draft(&mut self) -> Result<(), String> {
        let (Some(builder), Some(mut options)) = (self.builder.as_ref(), self.options.clone())
        else {
            return Err("the viewer is not building a schema".to_owned());
        };
        builder
            .write()
            .map_err(|e| format!("could not write the schema: {e}"))?;
        options.type_or_schema = builder.schema_path().to_string_lossy().into_owned();
        match load::try_load(&options) {
            Ok(loaded) => {
                if let Some(error) = &loaded.error {
                    return Err(format!("the file does not fit the schema: {error}"));
                }
                self.options = Some(options);
                if let Some(builder) = self.builder.as_mut() {
                    builder.draft.fit_to_trace(&loaded.trace);
                }
                self.adopt(loaded);
                self.notice = None;
                Ok(())
            }
            Err(failure) => Err(failure.error.to_string()),
        }
    }

    /// Parse with another type of the open schemas: the viewer reads the file
    /// again as `SCHEMA::TYPE` (`SCHEMA` alone for the schema's own type).
    pub fn choose_type(&mut self, name: &str, main_id: Option<&str>) {
        if self.loading {
            self.notice = Some("still reading the file".to_owned());
            return;
        }
        let Some(mut options) = self.options.clone() else {
            return;
        };
        let (schema, _) = doe::split_type_arg(&options.type_or_schema);
        let schema = schema.to_owned();
        options.type_or_schema = if main_id == Some(name) {
            schema
        } else {
            format!("{schema}::{name}")
        };
        match load::try_load(&options) {
            Ok(loaded) => {
                self.options = Some(options);
                self.adopt(loaded);
                self.notice = Some(format!("parsing as {name}"));
            }
            // the old type stays, with the old tree
            Err(failure) => self.reload_failed(&failure, &options),
        }
    }

    /// Text that is pasted: it goes into the prompt, if there is one.
    pub fn on_paste(&mut self, text: &str) {
        if let Some(prompt) = &mut self.prompt {
            prompt.line.insert_str(text);
        }
    }

    fn prompt_key(&mut self, key: &KeyEvent) {
        let Some(prompt) = self.prompt.as_mut() else {
            return;
        };
        match prompt.line.key(key) {
            PromptOutcome::Editing => {}
            PromptOutcome::Cancel => {
                self.prompt = None;
                self.field_dialog = None;
                self.group_plan = None;
                self.new_type_dialog = None;
            }
            PromptOutcome::Submit => {
                let Some(prompt) = self.prompt.take() else {
                    return;
                };
                match prompt.kind {
                    Kind::Search => self.start_search(prompt.line.text(), true),
                    Kind::Goto => self.goto(prompt.line.text()),
                    Kind::SaveEntry => self.save_entry(prompt.line.text()),
                    Kind::TypeName => self.type_name_answered(prompt.line.text()),
                    field => self.field_step(field, prompt.line.text()),
                }
            }
        }
    }

    fn open_prompt(&mut self, kind: Kind) {
        self.open_prompt_with(kind, "");
    }

    /// A prompt with a text in it that Enter accepts as it is.
    fn open_prompt_with(&mut self, kind: Kind, text: &str) {
        self.prompt = Some(Prompt {
            kind,
            line: LineEditor::with_text(text),
        });
    }

    /// Start a search for `query` from the selected field (the last query if it is empty).
    fn start_search(&mut self, query: &str, forward: bool) {
        let query = if query.is_empty() {
            self.last_query.clone().unwrap_or_default()
        } else {
            query.to_owned()
        };
        let Some(needle) = Needle::new(&query) else {
            self.notice = Some("nothing to search for".to_owned());
            return;
        };
        self.last_query = Some(query);
        self.search = Some(Search::new(
            needle,
            forward,
            self.tree.selected_path().to_vec(),
        ));
    }

    /// Look at the next [`SEARCH_STEP`] fields of the search that is going on.
    pub fn search_step(&mut self) {
        let Some(mut search) = self.search.take() else {
            return;
        };
        match search.step(&self.tree, SEARCH_STEP) {
            SearchOutcome::Running => self.search = Some(search),
            SearchOutcome::Found(path) => {
                self.tree.select_path(&path);
                self.show_selected_field();
                if search.wrapped() {
                    self.notice = Some("search wrapped around".to_owned());
                }
            }
            SearchOutcome::NotFound => {
                self.notice = Some(format!(
                    "not found: {}",
                    crate::format::safe(search.needle().query())
                ));
            }
        }
    }

    /// Go to the offset typed in `text`: the bytes show it and the tree selects the
    /// innermost field that holds it.
    fn goto(&mut self, text: &str) {
        let offset = match parse_offset(text) {
            Ok(offset) => offset,
            Err(message) => {
                self.notice = Some(message);
                return;
            }
        };
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        if offset >= size {
            self.notice = Some(format!(
                "offset {offset:#x} is past the end of the file ({size} bytes)"
            ));
            return;
        }
        let origin = self.here();
        match self.tree.find_byte(offset) {
            Some(path) => self.tree.select_path(&path),
            None => {
                self.notice = Some(format!(
                    "no field holds byte {offset:#x} (it is after what was parsed)"
                ));
            }
        }
        self.hex.reveal(offset, size);
        self.jumped(origin);
    }

    /// `J`: jump to what the selected field points at. A field that the schema's `pos:` or `size:` reads points at the
    /// value it places or sizes; an integer that is neither is a guess, the outermost value that starts at its number.
    fn follow_pointer(&mut self) {
        let row = self.tree.selected().clone();
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        let named = |app: &Self, path: &[usize]| {
            app.tree
                .node(&[])
                .map(|root| format::safe(&doe::copy::path_of(root, path)))
                .unwrap_or_default()
        };
        let known = self
            .tree
            .pointer_target(&row.path)
            .map(|(path, more)| (path.to_vec(), more));
        let (target, more, guess) = if let Some((path, more)) = known {
            (path, more, false)
        } else {
            let number = match row.shown {
                Some(crate::tree::Shown::UInt(n)) => Some(i128::from(n)),
                Some(crate::tree::Shown::SInt(n)) => Some(i128::from(n)),
                Some(crate::tree::Shown::Enum { value, .. }) => Some(value),
                _ => None,
            };
            let Some(number) = number else {
                self.notice = Some("this field does not point at anything".to_owned());
                return;
            };
            let Ok(offset) = u64::try_from(number) else {
                self.notice = Some(format!("{number} is not an offset"));
                return;
            };
            if offset >= size {
                self.notice = Some(format!(
                    "{offset:#x} is past the end of the file ({size} bytes)"
                ));
                return;
            }
            let found = self
                .tree
                .node(&[])
                .and_then(|root| root.starting_at(offset))
                .filter(|path| !path.is_empty() && *path != row.path);
            let Some(path) = found else {
                self.notice = Some(format!(
                    "nothing starts at {offset:#x}: the schema does not say that this field points anywhere"
                ));
                return;
            };
            (path, 0, true)
        };
        let Some(offset) = self
            .tree
            .node(&target)
            .and_then(|n| n.span)
            .map(|s| s.start_byte())
        else {
            self.notice = Some("what it points at is not in the file".to_owned());
            return;
        };
        if offset >= size {
            self.notice = Some(format!(
                "{offset:#x} is past the end of the file ({size} bytes)"
            ));
            return;
        }
        let origin = self.here();
        self.tree.select_path(&target);
        self.hex.reveal(offset, size);
        self.jumped(origin);
        self.notice = Some(format!(
            "→ {offset:#x}: {}{}{} (Ctrl-O comes back)",
            named(self, &target),
            if more > 0 {
                format!(" ({more} more are decided by this field)")
            } else {
                String::new()
            },
            if guess {
                " — a guess: a value starts here, the schema does not say that this field points at it"
            } else {
                ""
            }
        ));
    }

    /// Where the person is now: the byte under the cursor and the selected field.
    fn here(&self) -> crate::history::Place {
        crate::history::Place {
            offset: self.hex.cursor(),
            path: self.tree.selected_path().to_vec(),
        }
    }

    /// A jump from `origin` has been made: keep both places, unless it did not leave the screen.
    fn jumped(&mut self, origin: crate::history::Place) {
        self.jumps.leave(origin);
        let there = self.here();
        self.jumps.arrive(there, self.hex.screen_bytes());
    }

    /// Back or forward to the nearest place that still exists (a reload with another schema can have removed it).
    fn walk_history(&mut self, forward: bool) {
        let here = self.here();
        self.jumps.remember(here);
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        let mut skipped = 0;
        loop {
            let Some(place) = self.jumps.walk(forward) else {
                self.notice = Some(if skipped > 0 {
                    "the other places are gone".to_owned()
                } else if forward {
                    "this is the newest place".to_owned()
                } else {
                    "this is the oldest place: g and the jumps after it make more".to_owned()
                });
                return;
            };
            if place.offset >= size {
                self.jumps.drop_current();
                skipped += 1;
                continue;
            }
            if self.tree.node(&place.path).is_some() {
                self.tree.select_path(&place.path);
            } else if let Some(path) = self.tree.find_byte(place.offset) {
                self.tree.select_path(&path);
            }
            self.hex.reveal(place.offset, size);
            let name = self
                .tree
                .node(&[])
                .map_or_default(|root| doe::copy::path_of(root, self.tree.selected_path()));
            self.notice = Some(format!(
                "{} to {:#x} ({}){}",
                if forward { "forward" } else { "back" },
                place.offset,
                format::safe(&name),
                if skipped > 0 {
                    format!(", {skipped} gone")
                } else {
                    String::new()
                }
            ));
            return;
        }
    }

    fn act(&mut self, action: Action) {
        if !matches!(action, Action::HexNextField | Action::HexPrevField) {
            self.overlap = None;
        }
        match action {
            Action::Quit if self.focus == Focus::Hex && self.hex.selected_range().is_some() => {
                self.hex.clear_range();
            }
            Action::Quit => {
                if self.builder.as_ref().is_some_and(Builder::dirty) && !self.quit_was_armed {
                    self.quit_armed = true;
                    self.notice = Some(
                        "the schema is not saved: w saves it, q again leaves without saving it"
                            .to_owned(),
                    );
                } else {
                    self.quit = true;
                }
            }
            Action::ToggleFocus => {
                self.focus = match self.focus {
                    Focus::Tree => Focus::Hex,
                    Focus::Hex => Focus::Tree,
                };
                if self.focus == Focus::Hex {
                    // the cursor starts on the field that is selected
                    self.show_selected_field();
                }
            }
            Action::ToggleInspector => self.inspector = !self.inspector,
            Action::Files => self.toggle_files(),
            Action::Layers => self.cycle_layer(),
            Action::Fullscreen => {
                self.fullscreen = !self.fullscreen;
                self.notice = Some(if self.fullscreen {
                    match self.focus {
                        Focus::Tree => "the tree fills the screen (f or Esc puts both panes back)",
                        Focus::Hex => "the bytes fill the screen (f or Esc puts both panes back)",
                    }
                    .to_owned()
                } else {
                    "both panes are shown".to_owned()
                });
            }
            Action::Descriptions => {
                let on = !self.tree.docs_shown();
                self.tree.set_docs_shown(on);
                self.notice = Some(if on {
                    "the descriptions of the fields are shown after their values (D hides them)"
                        .to_owned()
                } else {
                    "the descriptions of the fields are hidden (D shows them)".to_owned()
                });
            }
            Action::ByteColours => {
                self.byte_colours = !self.byte_colours;
                self.notice = Some(if self.byte_colours {
                    "the bytes are coloured by the field they belong to (B turns it off)".to_owned()
                } else {
                    "the bytes are not coloured (B turns it on)".to_owned()
                });
            }
            Action::Help => {
                self.help_view.reset();
                self.help = true;
            }
            Action::ToggleReport => self.show_report = !self.show_report,
            Action::Reload if self.builder.is_some() => self.rename_field(),
            Action::Reload => self.reload(),
            Action::Undo
            | Action::Redo
            | Action::RemoveField
            | Action::ChangeKind
            | Action::ToggleByteOrder
            | Action::GroupFields
            | Action::UngroupField
            | Action::Save => self.builder_action(action),
            Action::TreeAddField => self.add_field_in(false, true),
            Action::Search => self.open_prompt(Kind::Search),
            Action::SearchNext | Action::SearchPrevious => match self.last_query.clone() {
                Some(query) => self.start_search(&query, action == Action::SearchNext),
                None => self.notice = Some("nothing searched for yet: press /".to_owned()),
            },
            Action::Goto => self.open_prompt(Kind::Goto),
            Action::FollowPointer => self.follow_pointer(),
            Action::JumpBack => self.walk_history(false),
            Action::JumpForward => self.walk_history(true),
            Action::Schemas => self.open_schemas(),
            Action::Types => self.open_types(),
            Action::Themes => self.open_themes(),
            Action::Radix => {
                self.radixes.cycle_global();
                self.notice = Some(format!(
                    "integers in {}{}",
                    self.radixes.global.name(),
                    if self.radixes.fields() > 0 {
                        " (except the fields that have a radix of their own)"
                    } else {
                        ""
                    }
                ));
            }
            Action::FieldRadix => self.cycle_field_radix(),
            Action::CopyAs => self.copy_as = true,
            Action::HexLeft
            | Action::HexRight
            | Action::HexUp
            | Action::HexDown
            | Action::HexPageUp
            | Action::HexPageDown
            | Action::HexLineStart
            | Action::HexLineEnd
            | Action::HexStart
            | Action::HexEnd
            | Action::HexBits
            | Action::HexNextField
            | Action::HexPrevField
            | Action::HexSelect
            | Action::HexCopy
            | Action::HexAddField
            | Action::HexAddFieldHere => self.hex_action(action),
            tree_action => {
                let before = self.tree.selected_path().to_vec();
                self.tree_action(tree_action);
                if self.tree.selected_path() != before.as_slice() {
                    self.show_selected_field();
                }
            }
        }
    }

    /// Scroll the bytes to the selected field, and put the cursor at its start.
    pub fn show_selected_field(&mut self) {
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        let row = self.tree.selected();
        if let Some(selection) = Selection::of(row) {
            self.hex.reveal(selection.first_byte(), size);
            if self.hex.bit().is_some() {
                self.hex.set_bit(u8::try_from(selection.start % 8).ok());
            }
        } else if let Some(span) = row.span.filter(|_| !row.processed) {
            // a field that failed has no bits, but it has a place
            self.hex.reveal(span.start / 8, size);
        }
    }

    /// Whether the files of an archive are shown in the place of the tree of fields.
    #[must_use]
    pub fn files_shown(&self) -> bool {
        self.files.is_some() && self.files_wanted
    }

    /// `F`: the files of an archive, or the tree of fields.
    fn toggle_files(&mut self) {
        if self.files.is_none() {
            self.notice =
                Some("this schema does not say that the file is a file system".to_owned());
            return;
        }
        self.files_wanted = !self.files_wanted;
        self.focus = Focus::Tree;
        if self.files_wanted {
            self.follow_tree_in_files();
        }
    }

    /// Select, in the files, the entry that holds the field that is selected in the tree.
    fn follow_tree_in_files(&mut self) {
        let path = self.tree.selected_path().to_vec();
        if path == self.files_followed {
            return;
        }
        if let Some(files) = self.files.as_mut() {
            files.select_trace(&path);
        }
        self.files_followed = path;
    }

    /// A key for the files: move, open, close; the entry that is selected is selected in the tree and the bytes follow.
    fn files_action(&mut self, action: Action) {
        match action {
            Action::Checksums => return self.cycle_checksums(true),
            Action::ChecksumsBack => return self.cycle_checksums(false),
            Action::CopyChecksum => return self.copy_checksum(),
            _ => {}
        }
        if matches!(action, Action::TreeToggle | Action::Preview) {
            let entry = self
                .files
                .as_ref()
                .and_then(FilesState::selected_entry)
                .filter(|entry| entry.kind != doe::filesystem::Kind::Directory)
                .cloned();
            match entry {
                Some(entry) => {
                    self.open_preview(&entry);
                    return;
                }
                None if action == Action::Preview => {
                    self.notice = Some("a directory has no preview: select a file".to_owned());
                    return;
                }
                None => {}
            }
        }
        let page = self.view.page();
        let Some(files) = self.files.as_mut() else {
            return;
        };
        match action {
            Action::TreeUp => files.up(),
            Action::TreeDown => files.down(),
            Action::TreePageUp => files.page_up(page),
            Action::TreePageDown => files.page_down(page),
            Action::TreeHome => files.home(),
            Action::TreeEnd => files.end(),
            Action::TreeRight => files.right(),
            Action::TreeLeft => files.left(),
            Action::TreeToggle => files.toggle(),
            _ => return,
        }
        let Some(path) = files.selected_entry().map(|e| e.trace_path.clone()) else {
            return;
        };
        self.tree.select_path(&path);
        self.files_followed = path;
        self.show_selected_field();
        // a file with no checksum says why on the status line
        if let Some(note) = self.files.as_ref().and_then(FilesState::checksum_note) {
            self.notice = Some(note);
        }
    }

    /// `m` goes to the next algorithm of the checksum column, MD5, SHA-1, and off after the last; `M` goes back. Showing one
    /// starts the thread that makes the checksums of it; what was made for the others stays.
    fn cycle_checksums(&mut self, forward: bool) {
        let Some(files) = self.files.as_mut() else {
            return;
        };
        // the positions are 0 for off and then one for each algorithm
        let all = Algorithm::ALL;
        let at = files
            .algorithm
            .and_then(|a| all.iter().position(|x| *x == a))
            .map_or(0, |i| i + 1);
        let count = all.len() + 1;
        let next = if forward {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        };
        files.algorithm = next.checked_sub(1).and_then(|i| all.get(i)).copied();
        match files.algorithm {
            Some(algorithm) => {
                let ranges = files.checksum_ranges();
                files.checksums.start(&self.file, algorithm, &ranges);
                self.notice = Some(format!(
                    "{} of each file: a checksum for comparing files, not a security measure (m next, M back)",
                    algorithm.label()
                ));
            }
            None => files.checksums.stop(),
        }
        if let Some(note) = self.files.as_ref().and_then(FilesState::checksum_note) {
            self.notice = Some(note);
        }
    }

    /// `y` in the files: the checksum of the selected file goes to the clipboard of the terminal.
    fn copy_checksum(&mut self) {
        match self
            .files
            .as_ref()
            .map(|f| (f.algorithm, f.selected_checksum()))
        {
            Some((Some(algorithm), Some(digits))) => {
                self.notice = Some(format!("copied the {} {digits}", algorithm.label()));
                self.clipboard = Some(digits);
            }
            Some((Some(_), None)) => {
                self.notice =
                    Some("this entry has no checksum yet (… or — in the column)".to_owned());
            }
            _ => self.notice = Some("the checksums are not shown: press m".to_owned()),
        }
    }

    /// Take the checksums that the thread has finished. True when the screen should be drawn again.
    pub fn poll_checksums(&mut self) -> bool {
        self.files
            .as_mut()
            .is_some_and(|files| files.checksums.poll())
    }

    fn tree_action(&mut self, action: Action) {
        if self.files_shown() {
            self.files_action(action);
            return;
        }
        let tree = &mut self.tree;
        match action {
            Action::TreeUp => tree.up(),
            Action::TreeDown => tree.down(),
            Action::TreePageUp => tree.page_up(self.view.page()),
            Action::TreePageDown => tree.page_down(self.view.page()),
            Action::TreeHome => tree.home(),
            Action::TreeEnd => tree.end(),
            Action::TreeRight => {
                if tree.selected().openable && !tree.selected().open {
                    tree.expand();
                } else if tree.selected().openable {
                    tree.first_child();
                } else {
                    // the "more" row shows the next piece
                    tree.expand();
                }
            }
            Action::TreeLeft => {
                if tree.selected().open {
                    tree.collapse();
                } else {
                    tree.parent();
                }
            }
            Action::TreeToggle => tree.toggle(),
            Action::NextSibling => tree.next_sibling(),
            Action::PreviousSibling => tree.prev_sibling(),
            Action::Checksums | Action::ChecksumsBack | Action::CopyChecksum => {
                self.notice = Some("the checksums are a column of the files: press F".to_owned());
            }
            Action::Preview => {
                self.notice = Some(if self.files.is_some() {
                    "the preview is of a file in the files: press F".to_owned()
                } else {
                    "there are no files to preview: the schema does not say this is a file system"
                        .to_owned()
                });
            }
            _ => {}
        }
    }

    /// Open the preview of an entry: its first bytes as text, highlighted. Says why if it cannot.
    fn open_preview(&mut self, entry: &doe::filesystem::Entry) {
        let Some((first, end)) = entry.data else {
            self.notice = Some(
                "this entry has no contents: the schema does not give them, or they are empty"
                    .to_owned(),
            );
            return;
        };
        let Some(bytes) = self.bytes.as_mut() else {
            self.notice = Some("the file cannot be read now".to_owned());
            return;
        };
        let want = usize::try_from(end.saturating_sub(first))
            .unwrap_or(usize::MAX)
            .min(PREVIEW_BYTES);
        let window = bytes.window(first, want).to_vec();
        if window.len() < want {
            self.notice = Some(
                "the bytes of this entry cannot be read (the file is shorter than the entry says)"
                    .to_owned(),
            );
            return;
        }
        let path = entry.path.join("/");
        let name = entry.path.last().cloned().unwrap_or_default();
        match Preview::new(path, name, (first, end), &window) {
            Ok(mut preview) => {
                preview.rendered = preview.is_markdown() && self.markdown_rendered;
                self.preview = Some(preview);
            }
            Err(message) => self.notice = Some(message),
        }
    }

    /// A key while the preview is open: scroll it, close it, or ask where to save the entry.
    fn preview_key(&mut self, key: &KeyEvent) {
        let Some(preview) = self.preview.as_mut() else {
            return;
        };
        let outcome = preview.key(key);
        // the choice between the source and the rendering is kept for the next Markdown entry
        if preview.is_markdown() {
            self.markdown_rendered = preview.rendered;
        }
        match outcome {
            PreviewOutcome::Keep => {}
            PreviewOutcome::Close => self.preview = None,
            PreviewOutcome::Save => {
                // a name that is only the entry's last component, never a path out of the folder
                let name = match preview.name.as_str() {
                    "" | "." | ".." => "entry".to_owned(),
                    name => name.replace(['/', '\\'], "_"),
                };
                self.open_prompt_with(Kind::SaveEntry, &name);
            }
        }
    }

    /// Write the contents of the entry that is previewed to the file `path` (not over a file that is there).
    fn save_entry(&mut self, path: &str) {
        let Some((first, end)) = self.preview.as_ref().map(|p| p.data) else {
            return;
        };
        if path.trim().is_empty() {
            self.notice = Some("nothing saved: no file name".to_owned());
            return;
        }
        let result = (|| -> std::io::Result<u64> {
            use std::io::{Read, Seek, SeekFrom};
            let mut input = std::fs::File::open(&self.file)?;
            input.seek(SeekFrom::Start(first))?;
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            let copied = std::io::copy(&mut input.take(end.saturating_sub(first)), &mut out)?;
            if copied != end.saturating_sub(first) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "the file is shorter than the entry says",
                ));
            }
            Ok(copied)
        })();
        self.notice = Some(match result {
            Ok(count) => format!("saved {count} bytes to {}", format::safe(path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                format!("not saved: {} is there already", format::safe(path))
            }
            Err(error) => format!("not saved: {}", format::safe(&error.to_string())),
        });
    }

    fn hex_action(&mut self, action: Action) {
        match action {
            Action::HexBits => return self.toggle_bits(),
            Action::HexNextField => return self.cycle_fields(1),
            Action::HexPrevField => return self.cycle_fields(-1),
            Action::HexSelect => return self.toggle_range(),
            Action::HexCopy => return self.copy_range(),
            Action::HexAddField => return self.add_field(false),
            Action::HexAddFieldHere => return self.add_field(true),
            _ => {}
        }
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        let line = i64::try_from(self.hex.bytes_per_line()).unwrap_or(16);
        let page = line.saturating_mul(i64::try_from(self.hex.page_lines()).unwrap_or(16));
        let cursor = self.hex.cursor();
        let width = u64::try_from(line).unwrap_or(16).max(1);
        let by_bits = self.hex.bit().is_some();
        match action {
            Action::HexLeft if by_bits => self.hex.move_bits(-1, size),
            Action::HexRight if by_bits => self.hex.move_bits(1, size),
            Action::HexLeft => self.hex.move_by(-1, size),
            Action::HexRight => self.hex.move_by(1, size),
            Action::HexUp => self.hex.move_by(-line, size),
            Action::HexDown => self.hex.move_by(line, size),
            Action::HexPageUp => self.hex.move_by(-page, size),
            Action::HexPageDown => self.hex.move_by(page, size),
            Action::HexLineStart => self.hex.set_cursor(cursor - cursor % width, size),
            Action::HexLineEnd => self
                .hex
                .set_cursor(cursor - cursor % width + width - 1, size),
            Action::HexStart => self.hex.set_cursor(0, size),
            Action::HexEnd => self.hex.set_cursor(size, size),
            _ => return,
        }
        self.follow_cursor();
    }

    /// The tree follows the cursor: the innermost field that holds the byte, or the
    /// bit when the cursor moves by bits.
    fn follow_cursor(&mut self) {
        let found = match self.hex.bit_position() {
            Some(bit) => self.tree.find_bit(bit),
            None => self.tree.find_byte(self.hex.cursor()),
        };
        match found {
            Some(path) => {
                if path.as_slice() != self.tree.selected_path() {
                    self.tree.select_path(&path);
                }
                if self.hex.bit().is_none() {
                    let holding = self.tree.find_all(self.hex.cursor()).len();
                    if holding > 1 {
                        self.notice = Some(format!(
                            "{holding} fields hold this byte: . and , go through them"
                        ));
                    }
                }
            }
            // what was parsed does not reach the byte: the selection stays where it was
            None => {
                self.notice = Some(format!(
                    "byte {:#x} is not in a field (it is after what was parsed)",
                    self.hex.cursor()
                ));
            }
        }
    }

    /// The kind (and what goes with it) is known: a new field needs a name, a field that
    /// changes kind is changed.
    fn after_kind(&mut self, dialog: FieldDialog) {
        match dialog.action {
            DialogAction::Retype(index) => match self.make_kind(&dialog) {
                Ok(kind) => self.finish_retype(&dialog, index, kind),
                Err(message) => self.notice = Some(message),
            },
            DialogAction::Add | DialogAction::Rename(_) => self.ask_name(dialog),
        }
    }

    /// Where the row of the tree at `path` is in the schema that is built: the schema or a type of it, and the index of the field.
    /// A field of a type is found through the fields that have the type: the row of a field that has a type of the schema has its
    /// fields under it (under each element, for a repeated one).
    fn resolve_path(&self, path: &[usize]) -> Option<(Container, usize)> {
        let draft = &self.builder.as_ref()?.draft;
        let mut container = Container::Top;
        let mut at = 0;
        loop {
            let index = *path.get(at)?;
            let field = draft.fields_in(&container).get(index)?;
            if at + 1 == path.len() {
                return Some((container, index));
            }
            match &field.kind {
                DraftKind::Type { id, .. } if draft.has_type(id) => {
                    container = Container::Type(id.clone());
                    at += 1;
                }
                // the row under a repeated field is an element, and the fields are under that
                DraftKind::Repeat { inner, .. } => match inner.as_ref() {
                    DraftKind::Type { id, .. } if draft.has_type(id) && path.len() > at + 2 => {
                        container = Container::Type(id.clone());
                        at += 2;
                    }
                    _ => return None,
                },
                _ => return None,
            }
        }
    }

    /// The field of the schema (or of a type of it) that is selected in the tree: where it is, its index and the row's path.
    fn selected_target(&mut self) -> Option<(Container, usize, Vec<usize>)> {
        let path = self.tree.selected_path().to_vec();
        let target = self
            .resolve_path(&path)
            .map(|(container, index)| (container, index, path));
        if target.is_none() {
            self.notice = Some(
                "select a field of the schema, or of a type it defines, first (a row of the tree that is a field)"
                    .to_owned(),
            );
        }
        target
    }

    /// Where the row at `path` starts in the file, in bytes.
    fn row_start(&self, path: &[usize]) -> u64 {
        self.tree
            .node(path)
            .and_then(|n| n.span)
            .map_or(0, |span| span.start / 8)
    }

    /// The keys that change the schema that is built: undo, redo, remove, rename, change the
    /// kind, and the byte order.
    fn builder_action(&mut self, action: Action) {
        if self.builder.is_none() {
            self.notice =
                Some("the schema is changed when the viewer is started with --build".to_owned());
            return;
        }
        let armed = self.remove_was_armed.take();
        match action {
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Save => {
                let result = self.builder.as_mut().map(Builder::save);
                self.notice = Some(match result {
                    Some(Ok(())) => {
                        let (out, count) = self.builder.as_ref().map_or_else(
                            || (String::new(), 0),
                            |b| (b.out.display().to_string(), b.draft.fields().len()),
                        );
                        format!(
                            "saved {} ({count} field{})",
                            crate::format::safe(&out),
                            if count == 1 { "" } else { "s" }
                        )
                    }
                    Some(Err(message)) => message,
                    None => String::new(),
                });
            }
            Action::ToggleByteOrder => {
                let result = self.edit_draft(|d| {
                    let to = d.endian().flipped();
                    Ok(d.set_endian(to))
                });
                self.notice = Some(match result {
                    Ok(()) => format!(
                        "the schema is {}-endian now",
                        self.builder
                            .as_ref()
                            .map_or("", |b| match b.draft.endian() {
                                crate::draft::Endian::Little => "little",
                                crate::draft::Endian::Big => "big",
                            })
                    ),
                    Err(message) => message,
                });
            }
            Action::RemoveField => {
                let Some((container, index, path)) = self.selected_target() else {
                    return;
                };
                let fields = self
                    .builder
                    .as_ref()
                    .map_or_default(|b| b.draft.fields_in(&container).to_vec());
                let after = fields.len().saturating_sub(index + 1);
                let name = fields.get(index).map_or_default(|f| f.id.clone());
                if after > 0 && armed != Some(index) {
                    self.remove_armed = Some(index);
                    self.notice = Some(format!(
                        "d again removes {name} and moves the {after} field{} after it up",
                        if after == 1 { "" } else { "s" }
                    ));
                    return;
                }
                match self.edit_draft(|d| d.remove_in(&container, index)) {
                    Ok(()) => {
                        self.select_field_near(&path, index);
                        self.notice = Some(format!("removed {name}"));
                    }
                    Err(message) => self.notice = Some(message),
                }
            }
            Action::GroupFields => self.group_fields(),
            Action::UngroupField => self.ungroup_field(),
            Action::ChangeKind => {
                let Some((container, index, path)) = self.selected_target() else {
                    return;
                };
                let Some(field) = self
                    .builder
                    .as_ref()
                    .and_then(|b| b.draft.fields_in(&container).get(index))
                    .cloned()
                else {
                    return;
                };
                let at = self.row_start(&path);
                let mut dialog = FieldDialog::new(DialogAction::Retype(index), at);
                dialog.container = container;
                dialog.path = path;
                // what the field is now is what is proposed
                match &field.kind {
                    DraftKind::Bytes(n) => dialog.selection = Some(*n),
                    DraftKind::Text { len, encoding } => {
                        dialog.selection = Some(*len);
                        dialog.encoding.clone_from(encoding);
                    }
                    _ => {}
                }
                self.field_dialog = Some(dialog);
                self.open_prompt_with(Kind::FieldKind, &field.kind.label());
            }
            _ => {}
        }
    }

    /// `r` while building: ask for another name for the selected field.
    fn rename_field(&mut self) {
        let Some((container, index, path)) = self.selected_target() else {
            return;
        };
        let name = self
            .builder
            .as_ref()
            .and_then(|b| b.draft.fields_in(&container).get(index))
            .map_or_default(|f| f.id.clone());
        let mut dialog = FieldDialog::new(DialogAction::Rename(index), 0);
        dialog.container = container;
        dialog.path = path;
        self.field_dialog = Some(dialog);
        self.open_prompt_with(Kind::FieldName, &name);
    }

    fn finish_rename(&mut self, dialog: &FieldDialog, index: usize, name: &str) {
        let container = dialog.container.clone();
        match self.edit_draft(|d| d.rename_in(&container, index, name)) {
            Ok(()) => {
                self.select_field_near(&dialog.path, index);
                self.notice = Some(format!("renamed to {name}"));
            }
            Err(message) => {
                self.notice = Some(message);
                // asked again, so that the name can be corrected
                self.field_dialog = Some(dialog.clone());
                self.open_prompt_with(Kind::FieldName, name);
            }
        }
    }

    fn finish_retype(&mut self, dialog: &FieldDialog, index: usize, kind: DraftKind) {
        let label = kind.label();
        let container = dialog.container.clone();
        match self.edit_draft(|d| d.retype_in(&container, index, kind)) {
            Ok(()) => {
                self.select_field_near(&dialog.path, index);
                self.notice = Some(format!("field {} is {label} now", index + 1));
            }
            Err(message) => self.notice = Some(message),
        }
    }

    /// Select the field at `index` of the container that the row at `path` is in, or the one before it if it was the last that
    /// was removed; with none left, the row that holds the container.
    fn select_field_near(&mut self, path: &[usize], index: usize) {
        let Some((container, _)) = self.resolve_path(path) else {
            self.tree.select_path(&[]);
            return;
        };
        let count = self
            .builder
            .as_ref()
            .map_or(0, |b| b.draft.fields_in(&container).len());
        let parent = path.split_last().map_or(&[][..], |(_, rest)| rest);
        let mut target = parent.to_vec();
        if count > 0 {
            target.push(index.min(count - 1));
        }
        self.tree.select_path(&target);
        self.show_selected_field();
    }

    /// After an edit is taken back or done again: the field it is about is selected, when that is a field of the schema.
    fn select_edit(&mut self, edit: &Edit) {
        let (Edit::Insert { at, index, .. }
        | Edit::Remove { at, index, .. }
        | Edit::Rename { at, index, .. }
        | Edit::Retype { at, index, .. }
        | Edit::Group {
            at, start: index, ..
        }
        | Edit::Ungroup {
            at, start: index, ..
        }) = edit
        else {
            return;
        };
        if *at == Container::Top {
            self.select_field_near(&[*index], *index);
        }
    }

    /// Take the last change to the schema back.
    fn undo(&mut self) {
        let Some(builder) = self.builder.as_mut() else {
            return;
        };
        let Some(edit) = builder.history.pop() else {
            self.notice = Some("nothing to undo".to_owned());
            return;
        };
        if let Err(error) = builder.draft.apply(&edit.inverse()) {
            self.notice = Some(error.to_string());
            return;
        }
        match self.parse_draft() {
            Ok(()) => {
                if let Some(builder) = self.builder.as_mut() {
                    builder.redo.push(edit.clone());
                }
                self.select_edit(&edit);
                self.notice = Some(format!("undid {}", edit.describe()));
            }
            Err(reason) => {
                if let Some(builder) = self.builder.as_mut() {
                    let _ = builder.draft.apply(&edit);
                    builder.history.push(edit);
                }
                let _ = self.parse_draft();
                self.notice = Some(reason);
            }
        }
    }

    /// Do again what was taken back.
    fn redo(&mut self) {
        let Some(builder) = self.builder.as_mut() else {
            return;
        };
        let Some(edit) = builder.redo.pop() else {
            self.notice = Some("nothing to do again".to_owned());
            return;
        };
        if let Err(error) = builder.draft.apply(&edit) {
            self.notice = Some(error.to_string());
            return;
        }
        match self.parse_draft() {
            Ok(()) => {
                if let Some(builder) = self.builder.as_mut() {
                    builder.history.push(edit.clone());
                }
                self.select_edit(&edit);
                self.notice = Some(format!("did again: {}", edit.describe()));
            }
            Err(reason) => {
                if let Some(builder) = self.builder.as_mut() {
                    let _ = builder.draft.apply(&edit.inverse());
                    builder.redo.push(edit);
                }
                let _ = self.parse_draft();
                self.notice = Some(reason);
            }
        }
    }

    /// Begin adding a field: where the schema ends (`a`), or at the cursor with a
    /// field of skipped bytes before it (`A`). The questions follow, one prompt each.
    fn add_field(&mut self, at_cursor: bool) {
        self.add_field_in(at_cursor, false);
    }

    /// Begin adding a field. With `into_selected` and a field of a type of the schema selected in the tree, the field goes into
    /// that type, after the fields it has, with the offsets measured from where an instance of the type starts.
    fn add_field_in(&mut self, at_cursor: bool, into_selected: bool) {
        if self.builder.is_none() {
            self.notice =
                Some("fields are added when the viewer is started with --build".to_owned());
            return;
        }
        if into_selected && let Some(dialog) = self.dialog_into_selected_type() {
            self.field_dialog = Some(dialog);
            self.open_prompt_with(Kind::FieldKind, &field_dialog::propose_kind(None));
            return;
        }
        let Some(builder) = self.builder.as_ref() else {
            return;
        };
        let end = builder.draft.end();
        let cursor = self.hex.cursor();
        let at = if at_cursor { cursor } else { end };
        if at < end {
            self.notice = Some(format!(
                "byte {at:#x} is in a field already: the schema reads up to {end:#x}"
            ));
            return;
        }
        let selection = self
            .hex
            .selected_range()
            .filter(|(first, _)| *first == at)
            .map(|(first, last)| last - first + 1);
        let mut dialog = FieldDialog::new(DialogAction::Add, at);
        dialog.gap = at - end;
        dialog.selection = selection;
        if selection.is_some() {
            dialog.selection_hex = self
                .selected_hex()
                .unwrap_or_default()
                .split(' ')
                .take(field_dialog::MAX_MAGIC)
                .collect::<Vec<_>>()
                .join(" ");
        }
        self.field_dialog = Some(dialog);
        self.open_prompt_with(Kind::FieldKind, &field_dialog::propose_kind(selection));
    }

    /// The questions for a field that goes into the type that the selected field has, if that is a type of the schema (as itself
    /// or as the element of a repeat).
    fn dialog_into_selected_type(&mut self) -> Option<FieldDialog> {
        let path = self.tree.selected_path().to_vec();
        let (container, index) = self.resolve_path(&path)?;
        let (type_id, repeated, start, end) = {
            let draft = &self.builder.as_ref()?.draft;
            let field = draft.fields_in(&container).get(index)?;
            let held = match &field.kind {
                DraftKind::Type { id, .. } => Some((id.clone(), false)),
                DraftKind::Repeat { inner, .. } => match inner.as_ref() {
                    DraftKind::Type { id, .. } => Some((id.clone(), true)),
                    _ => None,
                },
                _ => None,
            };
            let held = held.filter(|(id, _)| draft.has_type(id));
            let Some((id, repeated)) = held else {
                // a field that is in a type, and has no type of the schema itself: another field goes in the same type, after the
                // fields it has, where an instance of it starts
                let Container::Type(_) = &container else {
                    return None;
                };
                let holder = path.split_last().map(|(_, rest)| rest.to_vec())?;
                let end = draft.end_in(&container);
                let start = self.row_start(&holder);
                let mut dialog = FieldDialog::new(DialogAction::Add, start + end);
                dialog.container = container;
                dialog.path = holder;
                dialog.type_start = start;
                return Some(dialog);
            };
            let start = (container == Container::Top).then(|| draft.offset_of(index));
            let end = draft.end_in(&Container::Type(id.clone()));
            (id, repeated, start, end)
        };
        // where an instance starts: the field, and for a repeat its first element
        let mut holder = path;
        if repeated {
            holder.push(0);
        }
        let start = start.unwrap_or_else(|| self.row_start(&holder));
        let mut dialog = FieldDialog::new(DialogAction::Add, start + end);
        dialog.container = Container::Type(type_id);
        dialog.path = holder;
        dialog.type_start = start;
        Some(dialog)
    }

    /// An answer to one of the questions about a field: ask the next, or add it.
    fn field_step(&mut self, step: Kind, text: &str) {
        let Some(mut dialog) = self.field_dialog.take() else {
            return;
        };
        let refused = |app: &mut App, dialog: FieldDialog, message: String| {
            app.notice = Some(message);
            app.field_dialog = Some(dialog);
            app.open_prompt_with(step, text);
        };
        match step {
            Kind::FieldKind => {
                let word = text.trim();
                if word.eq_ignore_ascii_case(field_dialog::TYPE_WORD) {
                    self.pick_library_type(dialog);
                    return;
                }
                // a new type of the schema, empty: its fields are added after, into it
                if word.eq_ignore_ascii_case(field_dialog::NEW_WORD) {
                    if !matches!(dialog.action, DialogAction::Add) {
                        return refused(
                            self,
                            dialog,
                            "a new empty type is made when a field is added".to_owned(),
                        );
                    }
                    dialog.new_type = true;
                    self.new_type_dialog = Some(dialog);
                    self.open_prompt_with(Kind::TypeName, "");
                    return;
                }
                match field_dialog::parse_kind(text) {
                    Some(kind) => dialog.kind = kind,
                    // the id of a type of the type paths, typed
                    None => match self.library_type(word) {
                        Ok(Some(id)) => {
                            field_dialog::TYPE_WORD.clone_into(&mut dialog.kind);
                            dialog.type_id = id;
                        }
                        Ok(None) => {
                            return refused(
                                self,
                                dialog,
                                format!(
                                    "{text:?} is not a kind of field or a type of the type paths"
                                ),
                            );
                        }
                        Err(message) => return refused(self, dialog, message),
                    },
                }
            }
            Kind::FieldSize => match field_dialog::parse_size(text) {
                Ok(size) => dialog.size = size,
                Err(message) => return refused(self, dialog, message),
            },
            Kind::FieldLength => match field_dialog::parse_length(text) {
                Ok(n) => dialog.length = Some(n),
                Err(message) => return refused(self, dialog, message),
            },
            Kind::FieldEncoding => text.trim().clone_into(&mut dialog.encoding),
            Kind::FieldTerminator => match field_dialog::parse_byte(text) {
                Ok(byte) => dialog.terminator = byte,
                Err(message) => return refused(self, dialog, message),
            },
            Kind::FieldMagic => match field_dialog::parse_hex_bytes(text) {
                Ok(bytes) => dialog.magic = bytes,
                Err(message) => return refused(self, dialog, message),
            },
            Kind::FieldRepeat => match field_dialog::parse_repeat(text) {
                Ok(count) => dialog.repeat = count,
                Err(message) => return refused(self, dialog, message),
            },
            Kind::FieldName => {
                match dialog.action {
                    DialogAction::Add => self.finish_field(&dialog, text.trim()),
                    DialogAction::Rename(index) => self.finish_rename(&dialog, index, text.trim()),
                    DialogAction::Retype(_) => {}
                }
                return;
            }
            Kind::Search | Kind::Goto | Kind::SaveEntry | Kind::TypeName => return,
        }
        self.continue_field(dialog, step);
    }

    /// Ask what comes after the question `step`, or go on to the name when there is nothing more to ask.
    fn continue_field(&mut self, dialog: FieldDialog, step: Kind) {
        match field_dialog::next(&dialog.kind, step) {
            Some(next) => {
                let proposed = Self::propose_answer(next, &dialog);
                self.field_dialog = Some(dialog);
                self.open_prompt_with(next, &proposed);
            }
            None => self.after_kind(dialog),
        }
    }

    /// The id of a type of the type paths that a field can have: `Ok(None)` when `id` is not one. A type that takes
    /// parameters, and the schema being built itself, cannot be used.
    fn library_type(&self, id: &str) -> Result<Option<String>, String> {
        // a type that the schema defines is its own: it is not looked for on the type paths
        if self.builder.as_ref().is_some_and(|b| b.draft.has_type(id)) {
            return Ok(Some(id.to_owned()));
        }
        let Some(options) = self.options.as_ref() else {
            return Ok(None);
        };
        let Ok(found) = doe::config::discover_types(&options.include_paths) else {
            return Ok(None);
        };
        let Some(available) = found.iter().find(|t| t.id == id) else {
            return Ok(None);
        };
        if self.builder.as_ref().is_some_and(|b| b.draft.id() == id) {
            return Err(format!(
                "{id} is the schema that is being built: a schema cannot import itself"
            ));
        }
        let takes_parameters = std::fs::read_to_string(&available.source)
            .ok()
            .and_then(|text| doe::schema::raw::RawSchema::from_yaml_str(&text).ok())
            .is_some_and(|raw| !raw.params.is_empty());
        if takes_parameters {
            return Err(format!(
                "{id} takes parameters, which the builder cannot give yet: it is not offered"
            ));
        }
        Ok(Some(available.id.clone()))
    }

    /// The kind `type`: choose a type of the type paths in the browser of types, then go on with the questions.
    fn pick_library_type(&mut self, dialog: FieldDialog) {
        let include_paths = self
            .options
            .as_ref()
            .map_or_default(|o| o.include_paths.clone());
        let defined: Vec<String> = self
            .builder
            .as_ref()
            .map_or_default(|b| b.draft.types().iter().map(|t| t.id.clone()).collect());
        if include_paths.is_empty() && defined.is_empty() {
            self.notice = Some("there are no type paths: give one with -I".to_owned());
            self.field_dialog = Some(dialog);
            self.open_prompt_with(Kind::FieldKind, "");
            return;
        }
        self.types = Some(TypeBrowser::read(&include_paths).with_defined(&defined));
        self.notice = Some("choose the type of the field (Enter), or Esc for none".to_owned());
        self.field_pick = Some(dialog);
    }

    /// A type was chosen in the browser of types for a field, or none.
    fn picked_library_type(&mut self, chosen: Option<&str>) {
        let Some(mut dialog) = self.field_pick.take() else {
            return;
        };
        let Some(id) = chosen else {
            self.notice = Some("no type chosen".to_owned());
            self.field_dialog = Some(dialog);
            self.open_prompt_with(Kind::FieldKind, "");
            return;
        };
        match self.library_type(id) {
            Ok(Some(id)) => {
                field_dialog::TYPE_WORD.clone_into(&mut dialog.kind);
                dialog.type_id = id;
                self.continue_field(dialog, Kind::FieldKind);
            }
            Ok(None) => {
                self.notice = Some(format!("{id} is not a type of the type paths"));
                self.field_dialog = Some(dialog);
                self.open_prompt_with(Kind::FieldKind, "");
            }
            Err(message) => {
                self.notice = Some(message);
                self.field_dialog = Some(dialog);
                self.open_prompt_with(Kind::FieldKind, "");
            }
        }
    }

    /// What is proposed for a question: the selection's length and bytes, the usual encoding,
    /// the usual terminator.
    fn propose_answer(step: Kind, dialog: &FieldDialog) -> String {
        match step {
            Kind::FieldLength => dialog
                .selection
                .map_or_else(|| "1".to_owned(), |n| n.to_string()),
            Kind::FieldEncoding => dialog.encoding.clone(),
            Kind::FieldTerminator => dialog.terminator.to_string(),
            Kind::FieldMagic => dialog.selection_hex.clone(),
            _ => String::new(),
        }
    }

    /// The last question: the name, proposed from the kind.
    fn ask_name(&mut self, dialog: FieldDialog) {
        let from_type: String;
        let base = if field_dialog::bits_of(&dialog.kind).is_some() {
            "bits"
        } else if dialog.kind == field_dialog::TYPE_WORD {
            // the id of the type as a name: lower case, letters, digits and _
            from_type = dialog
                .type_id
                .to_ascii_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            from_type.as_str()
        } else {
            dialog.kind.as_str()
        };
        let proposed = self
            .builder
            .as_ref()
            .map_or_default(|b| field_dialog::propose_name(&b.draft, base));
        self.field_dialog = Some(dialog);
        self.open_prompt_with(Kind::FieldName, &proposed);
    }

    /// The kind of field that was asked for: what it is, and for text that ends and for
    /// repeats, how much of this file they take (the draft counts in bytes).
    fn make_kind(&mut self, dialog: &FieldDialog) -> Result<DraftKind, String> {
        let mut answers = field_dialog::Answers {
            length: dialog.length,
            encoding: dialog.encoding.clone(),
            magic: dialog.magic.clone(),
            terminator: dialog.terminator,
            text_sample: 0,
            type_id: dialog.type_id.clone(),
            size: dialog.size,
        };
        if dialog.kind == "strz" {
            answers.text_sample = self.measure_terminated(dialog.at, dialog.terminator)?;
        }
        let base = field_dialog::kind_of(&dialog.kind, &answers)?;
        let Some(count) = dialog.repeat.clone() else {
            return Ok(base);
        };
        let sample = match &count {
            Count::Fixed(n) => *n,
            Count::Field(name) => self.value_of_field(name, dialog)?,
            // the elements of a type are counted by the parse
            Count::ToEnd if matches!(base, DraftKind::Type { .. }) => 1,
            Count::ToEnd => {
                let size = self.bytes.as_ref().map_or(0, FileBytes::size);
                let each = base.size();
                let left = size.saturating_sub(dialog.at);
                if each == 0 || left == 0 || !left.is_multiple_of(each) {
                    return Err(format!(
                        "the {left} bytes from {:#x} to the end of the file are not a whole number of elements of {each} bytes",
                        dialog.at
                    ));
                }
                left / each
            }
        };
        Ok(DraftKind::Repeat {
            inner: Box::new(base),
            count,
            sample,
        })
    }

    /// How many bytes the text that starts at `at` takes, with its terminator.
    fn measure_terminated(&mut self, at: u64, terminator: u8) -> Result<u64, String> {
        let Some(bytes) = self.bytes.as_mut() else {
            return Err("the file cannot be read".to_owned());
        };
        let window = bytes.window(at, TERMINATOR_SEARCH);
        match window.iter().position(|b| *b == terminator) {
            Some(at_terminator) => Ok(at_terminator as u64 + 1),
            None => Err(format!(
                "no byte {terminator:#04x} from {at:#x} on (within {TERMINATOR_SEARCH} bytes): the text has to end with it"
            )),
        }
    }

    /// The number that the earlier integer field `name` has in this file.
    fn value_of_field(&self, name: &str, dialog: &FieldDialog) -> Result<u64, String> {
        if dialog.container != Container::Top {
            return Err("a repeat that follows the value of a field is made in the fields of the schema itself, not in a type yet: use a number or eos".to_owned());
        }
        let Some(builder) = self.builder.as_ref() else {
            return Err("the viewer is not building a schema".to_owned());
        };
        let limit = match dialog.action {
            DialogAction::Retype(index) => index,
            _ => builder.draft.fields().len(),
        };
        let Some(index) = builder
            .draft
            .fields()
            .iter()
            .take(limit)
            .position(|f| f.id == name)
        else {
            return Err(format!("there is no field called {name} before this one"));
        };
        let row = self
            .tree
            .rows()
            .iter()
            .find(|r| r.path == [index] && r.kind == crate::tree::RowKind::Node);
        match row.and_then(|r| r.shown.as_ref()) {
            Some(Shown::UInt(n)) => Ok(*n),
            Some(Shown::SInt(n)) if *n >= 0 => Ok(n.unsigned_abs()),
            _ => Err(format!(
                "{name} is not a number that is read from this file"
            )),
        }
    }

    /// Add the field that was asked for (and the skipped bytes before it), select it, and
    /// put the cursor after it.
    fn finish_field(&mut self, dialog: &FieldDialog, name: &str) {
        // an empty new type is made with its field
        if dialog.new_type {
            return self.finish_new_type(dialog, name);
        }
        let kind = match self.make_kind(dialog) {
            Ok(kind) => kind,
            Err(message) => {
                self.notice = Some(message);
                return;
            }
        };
        let field = Field {
            id: name.to_owned(),
            kind,
        };
        let container = dialog.container.clone();
        if dialog.gap > 0 {
            let skipped = self
                .builder
                .as_ref()
                .map_or_default(|b| field_dialog::propose_name(&b.draft, "skip"));
            let gap = Field {
                id: skipped,
                kind: DraftKind::Bytes(dialog.gap),
            };
            if let Err(message) = self.edit_draft(|d| d.push(gap)) {
                self.notice = Some(message);
                return;
            }
        }
        let size = field.kind.size();
        if let Err(message) = self.edit_draft(|d| {
            let end = d.fields_in(&container).len();
            d.insert_in(&container, end, field)
        }) {
            if dialog.gap > 0 {
                self.revert_last();
            }
            self.notice = Some(message);
            // asked again, so that a name that is taken can be corrected
            self.field_dialog = Some(dialog.clone());
            self.open_prompt_with(Kind::FieldName, name);
            return;
        }
        let Some(builder) = self.builder.as_ref() else {
            return;
        };
        if container != Container::Top {
            // a field of a type: it is selected in the tree under the field that has the type, and the cursor goes after it
            let index = builder.draft.fields_in(&container).len().saturating_sub(1);
            let mut path = dialog.path.clone();
            path.push(index);
            self.tree.select_path(&path);
            let at = dialog.type_start + builder.draft.offset_in(&container, index);
            let end = at + size;
            let file_size = self.bytes.as_ref().map_or(0, FileBytes::size);
            self.hex.clear_range();
            self.hex.set_cursor(end, file_size);
            self.notice = Some(format!(
                "added {name} to the type at {at:#x}; the type reads up to {end:#x} in this instance"
            ));
            return;
        }
        let index = builder.draft.fields().len().saturating_sub(1);
        let (end, at) = (builder.draft.end(), builder.draft.offset_of(index));
        let end_bit = builder.draft.end_bit();
        self.tree.select_path(&[index]);
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        self.hex.clear_range();
        if end_bit % 8 == 0 {
            self.hex.set_cursor(end, size);
            if self.hex.bit().is_some() {
                self.hex.set_bit(Some(0));
            }
        } else {
            // the next bit field goes on in this byte
            self.hex.set_cursor(end_bit / 8, size);
            self.hex.set_bit(u8::try_from(end_bit % 8).ok());
        }
        self.notice = Some(format!(
            "added {name} at {at:#x}; the schema reads up to {end:#x}"
        ));
    }

    /// The name of the new empty type was given: ask for the name of the field that has it.
    fn type_name_answered(&mut self, text: &str) {
        let name = text.trim().to_owned();
        // the fields of a group: the type is made from them
        if let Some((container, start, count, path)) = self.group_plan.take() {
            self.finish_group(&container, start, count, &path, &name);
            return;
        }
        let Some(mut dialog) = self.new_type_dialog.take() else {
            return;
        };
        let checked = self
            .builder
            .as_ref()
            .map(|b| b.draft.check_new_type_name(&name));
        if let Some(Err(e)) = checked {
            self.notice = Some(e.to_string());
            self.new_type_dialog = Some(dialog);
            self.open_prompt_with(Kind::TypeName, &name);
            return;
        }
        field_dialog::TYPE_WORD.clone_into(&mut dialog.kind);
        dialog.type_id.clone_from(&name);
        self.ask_name(dialog);
    }

    /// An empty type and a field that has it, at the end of the schema or of the type that the dialog is for.
    fn finish_new_type(&mut self, dialog: &FieldDialog, name: &str) {
        let container = dialog.container.clone();
        let type_id = dialog.type_id.clone();
        let field_id = name.to_owned();
        let result = self.edit_draft(|d| {
            let end = d.fields_in(&container).len();
            d.group_in(&container, end, 0, &type_id, &field_id)
        });
        match result {
            Ok(()) => {
                let index = self
                    .builder
                    .as_ref()
                    .map_or(0, |b| b.draft.fields_in(&container).len().saturating_sub(1));
                if container == Container::Top {
                    self.tree.select_path(&[index]);
                }
                self.notice = Some(format!(
                    "made the empty type {type_id} and the field {name}: select it and press a to add fields into the type"
                ));
            }
            Err(message) => {
                self.notice = Some(message);
                self.field_dialog = Some(dialog.clone());
                self.open_prompt_with(Kind::FieldName, name);
            }
        }
    }

    /// `Z` while building: the fields of the selected bytes, or the selected field, become a new type: ask for its name.
    fn group_fields(&mut self) {
        let Some(builder) = self.builder.as_ref() else {
            return;
        };
        let range = self.hex.selected_range();
        let (container, start, count, path) = if let Some((first, last)) = range {
            // the fields of the schema that the bytes are exactly (they are one after the other)
            let layout = builder.draft.layout();
            let covered: Vec<usize> = layout
                .iter()
                .enumerate()
                .filter(|(_, (a, b))| a / 8 >= first && b.div_ceil(8) <= last + 1)
                .map(|(i, _)| i)
                .collect();
            let (Some(&from), Some(&to)) = (covered.first(), covered.last()) else {
                self.notice = Some(format!(
                    "the selection has to start and end at the edges of fields: no field is between byte {first:#x} and {last:#x} (select the bytes of the fields with v)"
                ));
                return;
            };
            let exact = layout.get(from).is_some_and(|(a, _)| a / 8 == first)
                && layout
                    .get(to)
                    .is_some_and(|(_, b)| b.div_ceil(8) == last + 1);
            if !exact {
                self.notice = Some(format!(
                    "the selection has to start and end at the edges of fields: the fields it holds are {:#x}..{:#x}",
                    builder.draft.offset_of(from),
                    builder.draft.offset_of(to + 1)
                ));
                return;
            }
            (Container::Top, from, to - from + 1, Vec::new())
        } else {
            let Some((container, index, path)) = self.selected_target() else {
                return;
            };
            (container, index, 1, path)
        };
        self.group_plan = Some((container, start, count, path));
        let proposed = self.builder.as_ref().map_or_default(|b| {
            (1..=1000)
                .map(|n| format!("group_{n}"))
                .find(|name| b.draft.check_new_type_name(name).is_ok())
                .unwrap_or_default()
        });
        self.open_prompt_with(Kind::TypeName, &proposed);
    }

    fn finish_group(
        &mut self,
        container: &Container,
        start: usize,
        count: usize,
        path: &[usize],
        type_name: &str,
    ) {
        let field_id = self.builder.as_ref().map_or_default(|b| {
            let taken: Vec<String> = b
                .draft
                .fields_in(container)
                .iter()
                .map(|f| f.id.clone())
                .collect();
            (1..=1000)
                .map(|n| format!("{type_name}_{n}"))
                .find(|name| !taken.contains(name))
                .unwrap_or_default()
        });
        let container = container.clone();
        match self.edit_draft(|d| d.group_in(&container, start, count, type_name, &field_id)) {
            Ok(()) => {
                self.select_field_near(
                    &if path.is_empty() {
                        vec![start]
                    } else {
                        path.to_vec()
                    },
                    start,
                );
                self.notice = Some(format!(
                    "made {count} field{} the type {type_name}: the field {field_id} has it now",
                    if count == 1 { "" } else { "s" }
                ));
            }
            Err(message) => {
                self.notice = Some(message);
                // asked again, so that a name that is taken can be corrected
                self.group_plan = Some((container, start, count, path.to_vec()));
                self.open_prompt_with(Kind::TypeName, type_name);
            }
        }
    }

    /// `O` while building: the selected field has a type of the schema that no other field has: its fields take its place.
    fn ungroup_field(&mut self) {
        let Some((container, index, path)) = self.selected_target() else {
            return;
        };
        match self.edit_draft(|d| d.ungroup_in(&container, index)) {
            Ok(()) => {
                self.select_field_near(&path, index);
                self.notice =
                    Some("the type is taken apart: its fields are in the schema now".to_owned());
            }
            Err(message) => self.notice = Some(message),
        }
    }

    /// Take the last change to the draft back.
    fn revert_last(&mut self) {
        if let Some(builder) = self.builder.as_mut()
            && let Some(edit) = builder.history.pop()
        {
            let _ = builder.draft.apply(&edit.inverse());
        }
        let _ = self.parse_draft();
    }

    /// Start a selection of bytes at the cursor, or end the one there is.
    fn toggle_range(&mut self) {
        if self.hex.selected_range().is_some() {
            self.hex.clear_range();
        } else {
            self.hex.start_range();
            self.notice = Some("selecting: move to extend, v or q ends it, y copies it".to_owned());
        }
    }

    /// The selected bytes (at most [`COPY_BYTES`] of them), as hex.
    #[must_use]
    pub fn selected_hex(&mut self) -> Option<String> {
        let (first, last) = self.hex.selected_range()?;
        let len = usize::try_from(last - first + 1).unwrap_or(usize::MAX);
        let bytes = self.bytes.as_mut()?;
        let data = bytes.window(first, len.min(COPY_BYTES));
        Some(
            data.iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    /// Say the selected bytes as hex, and ask for them to be put on the clipboard.
    fn copy_range(&mut self) {
        let Some(hex) = self.selected_hex() else {
            self.notice = Some("nothing is selected: v starts a selection".to_owned());
            return;
        };
        self.notice = Some(format!("copied: {}", crate::format::truncate(&hex, 80)));
        self.clipboard = Some(hex);
    }

    /// The text that copying the selected field as `letter` makes (see [`COPY_FORMATS`]), or why it cannot be made.
    fn copy_text(&mut self, letter: char) -> Result<String, String> {
        let row = self.tree.selected().clone();
        let path = doe::copy::path_of(self.tree.node(&[]).ok_or("nothing is selected")?, &row.path);
        match letter {
            'p' => Ok(path),
            'v' => match &row.shown {
                // the text itself, without the quotes that the tree puts round it
                Some(crate::tree::Shown::Str { text, cut: false }) => Ok(text.clone()),
                _ => row
                    .summary
                    .clone()
                    .ok_or_else(|| "this has no single value: j copies it as JSON".to_owned()),
            },
            'j' => {
                let value = self
                    .tree
                    .value_at(&row.path)
                    .ok_or("the value was not kept, so there is no JSON of it")?;
                Ok(doe::render::json::render_pretty(value))
            }
            'x' | 'X' | 'c' => {
                if row.processed {
                    return Err("those bytes are decoded by a process: and are not in the file; o says where".to_owned());
                }
                let span = row.span.ok_or("this is computed and has no bytes")?;
                if !span.start.is_multiple_of(8) || !span.end.is_multiple_of(8) {
                    return Err("this is not a whole number of bytes: o says which bits".to_owned());
                }
                let len = usize::try_from((span.end - span.start) / 8).unwrap_or(usize::MAX);
                if len > COPY_LIMIT {
                    return Err(format!(
                        "{len} bytes is more than the {COPY_LIMIT} that can be copied here: save it with s in the files, or use doe --json"
                    ));
                }
                let bytes = self.bytes.as_mut().ok_or("the file is not open")?;
                let data = bytes.window(span.start / 8, len).to_vec();
                Ok(match letter {
                    'x' => doe::copy::hex_spaced(&data),
                    'X' => doe::copy::hex_continuous(&data),
                    _ => doe::copy::c_array(&path, &data),
                })
            }
            'o' => self
                .tree
                .node(&row.path)
                .and_then(doe::copy::place)
                .ok_or_else(|| "this is computed and is not in the file".to_owned()),
            's' => {
                let kind = schema_kind(&row);
                let parent = row
                    .path
                    .split_last()
                    .and_then(|(_, up)| self.tree.node(up))
                    .and_then(|n| {
                        n.type_name
                            .as_deref()
                            .map(|t| t.rsplit("::").next().unwrap_or(t).to_owned())
                    });
                Ok(doe::copy::schema_field(&row.name, &kind, parent.as_deref()))
            }
            other => Err(format!("{other} is not one of the choices")),
        }
    }

    /// Copy the selected field as `letter` says, or say why not.
    fn copy_as_letter(&mut self, letter: char) {
        match self.copy_text(letter) {
            Ok(text) if text.len() > COPY_LIMIT => {
                self.notice = Some(format!(
                    "{} bytes of text is more than the {COPY_LIMIT} that can be copied here: use doe --json",
                    text.len()
                ));
            }
            Ok(text) => {
                self.notice = Some(format!(
                    "copied: {}",
                    crate::format::truncate(&crate::format::safe(&text.replace('\n', " ")), 100)
                ));
                self.clipboard = Some(text);
            }
            Err(message) => self.notice = Some(message),
        }
    }

    /// The text to put on the clipboard, if `y` asked for it since the last call.
    pub fn take_clipboard(&mut self) -> Option<String> {
        self.clipboard.take()
    }

    /// Move by bits, or by bytes again. Going to bits starts on the first bit of the
    /// selected field when it begins in this byte.
    fn toggle_bits(&mut self) {
        if self.hex.bit().is_some() {
            self.hex.set_bit(None);
            self.notice = Some("moving by bytes".to_owned());
            return;
        }
        let start = Selection::of(self.tree.selected())
            .filter(|s| s.first_byte() == self.hex.cursor())
            .and_then(|s| u8::try_from(s.start % 8).ok())
            .unwrap_or(0);
        self.hex.set_bit(Some(start));
        self.follow_cursor();
        self.notice = Some("moving by bits: ← → move one bit, b goes back to bytes".to_owned());
    }

    /// Select the next (or previous) field that holds the byte under the cursor.
    fn cycle_fields(&mut self, step: isize) {
        let offset = self.hex.cursor();
        let mut overlap = match self.overlap.take() {
            Some(o) if o.offset == offset => o,
            _ => {
                let paths = self.tree.find_all(offset);
                let at = paths
                    .iter()
                    .position(|p| p.as_slice() == self.tree.selected_path())
                    .unwrap_or(0);
                Overlap { offset, paths, at }
            }
        };
        let count = overlap.paths.len();
        if count == 0 {
            self.notice = Some(format!("byte {offset:#x} is not in a field"));
            return;
        }
        overlap.at = overlap
            .at
            .saturating_add_signed(step.rem_euclid(isize::try_from(count).unwrap_or(1)))
            % count;
        if let Some(path) = overlap.paths.get(overlap.at) {
            self.tree.select_path(path);
        }
        self.notice = Some(if count == 1 {
            format!("only one field holds byte {offset:#x}")
        } else {
            format!(
                "field {} of {count} that hold byte {offset:#x}: {}",
                overlap.at + 1,
                crate::format::safe(&self.tree.selected().name)
            )
        });
        self.overlap = Some(overlap);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;
    use crate::Options;
    use crate::load::load;

    /// A viewer for a three-byte file and a schema for it.
    pub(crate) fn app() -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let schema = dir.path().join("t.yaml");
        std::fs::write(
            &schema,
            "id: t\nseq:\n  - id: a\n    type: u1\n  - id: b\n    type: u2le\n",
        )
        .unwrap();
        let file = dir.path().join("t.bin");
        std::fs::write(&file, [7_u8, 0x34, 0x12]).unwrap();
        let options = Options {
            file: file.clone(),
            type_or_schema: schema.to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
            config_file: None,
            outer_file: None,
            theme_on_command_line: false,
            transparent: None,
            radix: None,
        };
        (App::new(file, load(&options).unwrap()), dir)
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.on_key(KeyEvent::new(code, modifiers));
    }

    #[test]
    fn q_escape_and_ctrl_c_leave() {
        for (code, modifiers) in [
            (KeyCode::Char('q'), KeyModifiers::NONE),
            (KeyCode::Esc, KeyModifiers::NONE),
            (KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let (mut app, _dir) = app();
            assert!(!app.quit);
            press(&mut app, code, modifiers);
            assert!(app.quit, "{code:?}");
        }
    }

    fn key(app: &mut App, code: KeyCode) {
        press(app, code, KeyModifiers::NONE);
    }

    /// A key without modifiers, for other tests.
    pub(crate) fn press_key(app: &mut App, code: KeyCode) {
        key(app, code);
    }

    /// A viewer for `data` parsed with `schema`.
    pub(crate) fn app_for(schema: &str, data: &[u8]) -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let schema_path = dir.path().join("t.yaml");
        std::fs::write(&schema_path, schema).unwrap();
        let file = dir.path().join("t.bin");
        std::fs::write(&file, data).unwrap();
        let options = Options {
            file: file.clone(),
            type_or_schema: schema_path.to_string_lossy().into_owned(),
            include_paths: vec![dir.path().to_path_buf()],
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
            config_file: None,
            outer_file: None,
            theme_on_command_line: false,
            transparent: None,
            radix: None,
        };
        let loaded = load(&options).unwrap();
        let mut app = App::new(file, loaded);
        app.options = Some(options);
        (app, dir)
    }

    /// Fields to search: a name, a count, and three items with an enum and a label.
    const REC: &str = "id: rec
seq:
  - id: name
    type: str
    size: 5
    encoding: UTF-8
  - id: Count
    type: u1
  - id: items
    type: item
    repeat: expr
    repeat-expr: 3
types:
  item:
    seq:
      - id: kind
        type: u1
        enum: kinds
      - id: label
        type: str
        size: 2
        encoding: UTF-8
enums:
  kinds:
    1: alpha
    2: beta
";

    fn rec_data() -> Vec<u8> {
        let mut data = b"Hello".to_vec();
        data.push(7);
        for (kind, label) in [(1_u8, "aa"), (2, "bB"), (1, "cc")] {
            data.push(kind);
            data.extend(label.as_bytes());
        }
        data
    }

    /// Press `Y` and then `letter`, and what went to the clipboard.
    fn copied(app: &mut App, letter: char) -> Option<String> {
        app.take_clipboard();
        key(app, KeyCode::Char('Y'));
        assert!(app.copy_as);
        key(app, KeyCode::Char(letter));
        assert!(!app.copy_as);
        app.take_clipboard()
    }

    #[test]
    fn y_copies_the_selected_field_in_eight_ways() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        app.tree.select_path(&[2, 1, 1]);
        assert_eq!(copied(&mut app, 'p').as_deref(), Some("items[1].label"));
        assert_eq!(copied(&mut app, 'v').as_deref(), Some("bB"));
        assert_eq!(copied(&mut app, 'x').as_deref(), Some("62 42"));
        assert_eq!(copied(&mut app, 'X').as_deref(), Some("6242"));
        assert_eq!(
            copied(&mut app, 'c').as_deref(),
            Some("unsigned char items_1_label[] = { 0x62, 0x42 };")
        );
        assert_eq!(copied(&mut app, 'o').as_deref(), Some("0xa+2"));
        assert_eq!(copied(&mut app, 's').as_deref(), Some("label: str in item"));
        assert!(copied(&mut app, 'j').unwrap().contains("bB"));
        // the path finds the field again
        let root = app.tree.node(&[]).unwrap();
        assert_eq!(
            doe::copy::resolve(root, "items[1].label"),
            Some(vec![2, 1, 1])
        );
        // a letter that is not a choice, and Esc, copy nothing
        assert_eq!(copied(&mut app, 'q'), None);
        key(&mut app, KeyCode::Char('Y'));
        key(&mut app, KeyCode::Esc);
        assert!(!app.copy_as);
        assert_eq!(app.take_clipboard(), None);
        // the root is $
        app.tree.select_path(&[]);
        assert_eq!(copied(&mut app, 'p').as_deref(), Some("$"));
        assert_eq!(copied(&mut app, 'v'), None);
    }

    #[test]
    fn g_records_the_jump_and_ctrl_o_and_ctrl_n_walk_it() {
        let schema = "id: big\nseq:\n  - id: head\n    type: u1\n  - id: body\n    size: 4000\n  - id: tail\n    type: u1\n";
        let (mut app, _dir) = app_for(schema, &vec![7_u8; 4002]);
        let ctrl = |app: &mut App, c: char| {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
        };
        let alt = |app: &mut App, code: KeyCode| app.on_key(KeyEvent::new(code, KeyModifiers::ALT));
        // nothing to go back to yet
        ctrl(&mut app, 'o');
        assert!(app.notice.as_deref().unwrap().contains("oldest"));
        app.goto("0x800");
        app.goto("0xe00");
        // a hop within a screen is no place of its own
        app.goto("0xe10");
        assert_eq!(app.hex.cursor(), 0xe10);
        ctrl(&mut app, 'o');
        assert_eq!(app.hex.cursor(), 0x800);
        assert!(app.notice.as_deref().unwrap().starts_with("back to 0x800"));
        alt(&mut app, KeyCode::Left);
        assert_eq!(app.hex.cursor(), 0);
        ctrl(&mut app, 'o');
        assert!(app.notice.as_deref().unwrap().contains("oldest"));
        assert_eq!(app.hex.cursor(), 0);
        ctrl(&mut app, 'n');
        assert_eq!(app.hex.cursor(), 0x800);
        alt(&mut app, KeyCode::Right);
        assert_eq!(app.hex.cursor(), 0xe10);
        ctrl(&mut app, 'n');
        assert!(app.notice.as_deref().unwrap().contains("newest"));
        // walking back and jumping again drops what was ahead; arrows record nothing
        ctrl(&mut app, 'o');
        app.goto("0x200");
        ctrl(&mut app, 'n');
        assert!(app.notice.as_deref().unwrap().contains("newest"));
        key(&mut app, KeyCode::Down);
        ctrl(&mut app, 'o');
        assert_eq!(app.hex.cursor(), 0x800);
    }

    #[test]
    fn j_follows_a_pointer_and_ctrl_o_comes_back() {
        let schema = "id: p\nmeta:\n  endian: le\nseq:\n  - id: ofs\n    type: u2\n  - id: other\n    type: u2\n  - id: gap\n    size: 0x7fc\ninstances:\n  table:\n    pos: ofs\n    type: u1\n    repeat: expr\n    repeat-expr: 2\n";
        let mut data = vec![0_u8; 0x802];
        data[0] = 0x00;
        data[1] = 0x08;
        data[2] = 0xff;
        data[3] = 0xff;
        let (mut app, _dir) = app_for(schema, &data);
        // the field that places `table` shows where
        app.tree.select_path(&[0]);
        assert_eq!(app.tree.selected().points_to, Some(0x800));
        assert_eq!(app.tree.selected_path(), [0]);
        key(&mut app, KeyCode::Char('J'));
        assert_eq!(app.hex.cursor(), 0x800);
        assert_eq!(app.tree.names_to(app.tree.selected_path()), ["p", "table"]);
        assert!(app.notice.as_deref().unwrap().starts_with("→ 0x800: table"));
        app.on_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
        assert_eq!(app.hex.cursor(), 0);
        assert_eq!(app.tree.selected_path(), [0]);
        // a number that points nowhere in the file is said, and nothing moves
        app.tree.select_path(&[1]);
        key(&mut app, KeyCode::Char('J'));
        assert!(app.notice.as_deref().unwrap().contains("past the end"));
        assert_eq!(app.hex.cursor(), 0);
        // a field that is not a number points at nothing
        app.tree.select_path(&[]);
        key(&mut app, KeyCode::Char('J'));
        assert!(app.notice.as_deref().unwrap().contains("does not point"));
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            key(app, KeyCode::Char(c));
        }
    }

    /// Run the search that is going on to its end, as the event loop would.
    fn finish_search(app: &mut App) {
        let mut steps = 0;
        while app.searching() {
            app.search_step();
            steps += 1;
            assert!(steps < 100, "the search never ends");
        }
    }

    fn selected(app: &App) -> Vec<String> {
        app.tree.names_to(app.tree.selected_path())
    }

    #[test]
    fn offsets_are_decimal_or_hex() {
        assert_eq!(parse_offset("0"), Ok(0));
        assert_eq!(parse_offset(" 42 "), Ok(42));
        assert_eq!(parse_offset("0x2a"), Ok(42));
        assert_eq!(parse_offset("0X2A"), Ok(42));
        assert_eq!(parse_offset("1_000"), Ok(1000));
        assert_eq!(parse_offset("0xffff_ffff_ffff_ffff"), Ok(u64::MAX));
        for bad in ["", "  ", "abc", "0x", "-5", "+5", "0xg", "1.5", "0b11"] {
            assert!(parse_offset(bad).is_err(), "{bad:?}");
        }
        assert!(
            parse_offset("18446744073709551616")
                .unwrap_err()
                .contains("too large")
        );
        // what is wrong is said without printing raw text
        assert!(!parse_offset("a\x1bb").unwrap_err().contains('\x1b'));
    }

    #[test]
    fn searching_with_the_prompt_selects_the_match_and_shows_its_bytes() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('/'));
        assert!(app.prompt.is_some());
        type_text(&mut app, "beta");
        key(&mut app, KeyCode::Enter);
        assert!(app.prompt.is_none() && app.searching());
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[1]", "kind"]);
        // the bytes follow: kind of the second item is byte 9
        assert_eq!(app.hex.cursor(), 9);
        assert_eq!(app.last_query.as_deref(), Some("beta"));
        // n goes on, round to the same one (it is the only match), and says so
        key(&mut app, KeyCode::Char('n'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[1]", "kind"]);
        assert_eq!(app.notice.as_deref(), Some("search wrapped around"));
    }

    #[test]
    fn n_and_capital_n_walk_through_the_matches_both_ways() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "label");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[0]", "label"]);
        key(&mut app, KeyCode::Char('n'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[1]", "label"]);
        key(&mut app, KeyCode::Char('n'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[2]", "label"]);
        key(&mut app, KeyCode::Char('N'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[1]", "label"]);
        // round the other way
        key(&mut app, KeyCode::Char('N'));
        finish_search(&mut app);
        key(&mut app, KeyCode::Char('N'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[2]", "label"]);
        assert_eq!(app.notice.as_deref(), Some("search wrapped around"));
    }

    #[test]
    fn a_search_with_no_match_says_so_and_changes_nothing() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Down);
        let before = selected(&app);
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "no such thing");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        assert_eq!(app.notice.as_deref(), Some("not found: no such thing"));
        assert_eq!(selected(&app), before);
        // case: a capital in the query makes it exact
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "HELLO");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        assert!(app.notice.as_deref().unwrap().starts_with("not found"));
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "hello");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "name"]);
    }

    #[test]
    fn an_empty_query_repeats_the_last_one_and_n_needs_one() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('n'));
        assert_eq!(
            app.notice.as_deref(),
            Some("nothing searched for yet: press /")
        );
        key(&mut app, KeyCode::Char('/'));
        key(&mut app, KeyCode::Enter); // nothing typed, nothing before
        assert_eq!(app.notice.as_deref(), Some("nothing to search for"));
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "kind");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        let first = selected(&app);
        key(&mut app, KeyCode::Char('/'));
        key(&mut app, KeyCode::Enter); // the same query again, from here
        finish_search(&mut app);
        assert_ne!(selected(&app), first);
    }

    #[test]
    fn a_key_stops_a_search_that_is_going_on_and_is_then_used() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        app.search = Some(Search::new(Needle::new("beta").unwrap(), true, Vec::new()));
        assert!(app.searching());
        key(&mut app, KeyCode::Down); // stops it, and moves
        assert!(!app.searching());
        assert_eq!(app.notice, Some("search stopped".to_owned()));
        assert_eq!(selected(&app), ["rec", "name"]);
        // Esc and Ctrl-C only stop it: they do not leave
        app.search = Some(Search::new(Needle::new("beta").unwrap(), true, Vec::new()));
        key(&mut app, KeyCode::Esc);
        assert!(!app.searching() && !app.quit);
        app.search = Some(Search::new(Needle::new("beta").unwrap(), true, Vec::new()));
        press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!app.searching() && !app.quit);
        key(&mut app, KeyCode::Esc);
        assert!(app.quit);
    }

    #[test]
    fn goto_selects_the_innermost_field_at_an_offset() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        // 0x7 is the first byte of the label of the first item
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "0x7");
        key(&mut app, KeyCode::Enter);
        assert_eq!(selected(&app), ["rec", "items", "[0]", "label"]);
        assert_eq!(app.hex.cursor(), 7);
        // decimal; a byte of the last label
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "13");
        key(&mut app, KeyCode::Enter);
        assert_eq!(selected(&app), ["rec", "items", "[2]", "label"]);
        assert_eq!(app.hex.cursor(), 13);
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "0");
        key(&mut app, KeyCode::Enter);
        assert_eq!(selected(&app), ["rec", "name"]);
        assert!(app.notice.is_none());
    }

    #[test]
    fn goto_out_of_the_file_or_with_a_bad_offset_says_so_and_stays() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Down);
        let before = (selected(&app), app.hex.cursor());
        for (typed, said) in [
            ("15", "offset 0xf is past the end of the file (15 bytes)"),
            (
                "0xffff",
                "offset 0xffff is past the end of the file (15 bytes)",
            ),
            ("nope", "not an offset: nope"),
            ("", "type an offset: a number, or 0x… for hex"),
            ("-1", "not an offset: -1"),
        ] {
            key(&mut app, KeyCode::Char('g'));
            type_text(&mut app, typed);
            key(&mut app, KeyCode::Enter);
            assert_eq!(app.notice.as_deref(), Some(said));
            assert_eq!((selected(&app), app.hex.cursor()), before);
        }
    }

    #[test]
    fn goto_in_a_gap_between_fields_selects_the_type_that_holds_it_or_nothing() {
        // `inner` is 4 bytes but its fields read 2: bytes 2 and 3 are in no field of it
        let schema = "id: gap\nseq:\n  - id: inner\n    type: pair\n    size: 4\ntypes:\n  pair:\n    seq:\n      - id: x\n        type: u1\n      - id: y\n        type: u1\n";
        let (mut app, _dir) = app_for(schema, &[1, 2, 3, 4]);
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "3");
        key(&mut app, KeyCode::Enter);
        assert_eq!(
            selected(&app),
            ["gap", "inner"],
            "the type that holds the byte"
        );
        assert_eq!(app.hex.cursor(), 3);
        // bytes after what the type reads: no field holds them
        let schema = "id: short\nseq:\n  - id: a\n    type: u2le\n";
        let (mut app, _dir) = app_for(schema, &[1, 2, 3, 4, 5]);
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "4");
        key(&mut app, KeyCode::Enter);
        assert_eq!(
            app.notice.as_deref(),
            Some("no field holds byte 0x4 (it is after what was parsed)")
        );
        assert_eq!(selected(&app), ["short"], "the selection is as it was");
        assert_eq!(app.hex.cursor(), 4, "the bytes show it");
    }

    #[test]
    fn the_prompt_takes_keys_as_text_and_pastes_and_cancels() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('/'));
        // keys that do things elsewhere are text here
        type_text(&mut app, "qig?/");
        assert!(!app.quit && !app.inspector && !app.help);
        assert_eq!(app.prompt.as_ref().unwrap().line.text(), "qig?/");
        key(&mut app, KeyCode::Left);
        key(&mut app, KeyCode::Backspace);
        assert_eq!(app.prompt.as_ref().unwrap().line.text(), "qig/");
        app.on_paste("beta\nsecond line");
        assert_eq!(app.prompt.as_ref().unwrap().line.text(), "qigbeta/");
        // Esc leaves the prompt, not the viewer
        key(&mut app, KeyCode::Esc);
        assert!(app.prompt.is_none() && !app.quit);
        // a paste with no prompt goes nowhere
        app.on_paste("beta");
        assert!(app.prompt.is_none());
        // the keys of the tree work again
        key(&mut app, KeyCode::Down);
        assert_eq!(selected(&app), ["rec", "name"]);
    }

    #[test]
    fn question_mark_shows_the_help_and_the_next_key_closes_it_and_does_nothing_else() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('?'));
        assert!(app.help);
        key(&mut app, KeyCode::Char('q'));
        assert!(
            !app.help && !app.quit,
            "the key closed the help and was not used"
        );
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }

    #[test]
    fn brackets_go_to_the_next_and_previous_field_at_the_same_level() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Down); // name
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(selected(&app), ["rec", "Count"]);
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(selected(&app), ["rec", "items"]);
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(selected(&app), ["rec", "items"], "no field after the last");
        key(&mut app, KeyCode::Char('['));
        key(&mut app, KeyCode::Char('['));
        assert_eq!(selected(&app), ["rec", "name"]);
        // the bytes follow
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(app.hex.cursor(), 5);
    }

    /// A parse that fails in a type inside a type: `body` needs three bytes and has two.
    const NESTED_FAIL: &str = "id: rec
seq:
  - id: head
    type: u1
  - id: body
    type: pair
types:
  pair:
    seq:
      - id: x
        type: u1
      - id: y
        type: u2le
";

    /// A parse that fails in the third element of a repeated field.
    const REPEATED_FAIL: &str = "id: rec
seq:
  - id: count
    type: u1
  - id: items
    type: u2le
    repeat: expr
    repeat-expr: count
";

    #[test]
    fn a_parse_that_fails_in_a_nested_type_selects_the_field_and_shows_its_bytes() {
        let (app, _dir) = app_for(NESTED_FAIL, &[1, 5, 9]);
        assert!(app.error.is_some());
        let report = app.report.as_ref().unwrap();
        assert!(!report.fatal);
        assert_eq!(
            report.headline,
            "the parse failed at rec/body/y (in rec::pair)"
        );
        assert_ne!(report.causes.len(), 0);
        // the failed field is selected, marked, and the bytes are at it
        assert_eq!(selected(&app), ["rec", "body", "y"]);
        assert!(app.tree.selected().failed);
        assert_eq!(app.hex.cursor(), 2);
        // what was read before is in the tree
        assert!(
            app.tree
                .rows()
                .iter()
                .any(|r| r.name == "x" && r.shown.is_some())
        );
        assert!(app.show_report);
    }

    #[test]
    fn a_parse_that_fails_in_a_repeated_field_selects_the_element() {
        let (app, _dir) = app_for(REPEATED_FAIL, &[3, 1, 0, 2, 0, 9]);
        let report = app.report.as_ref().unwrap();
        assert_eq!(
            report.headline,
            "the parse failed at rec/items/[2] (in rec)"
        );
        assert_eq!(selected(&app), ["rec", "items", "[2]"]);
        assert!(app.tree.selected().failed);
        assert_eq!(app.hex.cursor(), 5);
        // the two elements that were read are there
        assert!(app.tree.rows().iter().any(|r| r.name == "[1]"));
    }

    #[test]
    fn a_parse_that_works_has_no_report_and_the_viewer_ends_well() {
        let (app, _dir) = app();
        assert!(app.report.is_none());
        assert_eq!(app.finish_message(), None);
    }

    #[test]
    fn what_is_said_when_the_viewer_is_left_names_the_file_and_the_error() {
        let (app, dir) = app_for(NESTED_FAIL, &[1, 5, 9]);
        let message = app.finish_message().unwrap();
        let file = dir.path().join("t.bin").display().to_string();
        assert!(
            message.starts_with(&format!(
                "{file}: the parse failed at rec/body/y (in rec::pair): "
            )),
            "{message}"
        );
    }

    #[test]
    fn e_shows_and_hides_the_message() {
        let (mut app, _dir) = app_for(NESTED_FAIL, &[1, 5, 9]);
        assert!(app.show_report);
        key(&mut app, KeyCode::Char('e'));
        assert!(!app.show_report);
        key(&mut app, KeyCode::Char('e'));
        assert!(app.show_report);
    }

    fn broken_app(schema: &str, data: Option<&[u8]>) -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let schema_path = dir.path().join("t.yaml");
        std::fs::write(&schema_path, schema).unwrap();
        let file = dir.path().join("t.bin");
        if let Some(data) = data {
            std::fs::write(&file, data).unwrap();
        }
        let options = Options {
            file: file.clone(),
            type_or_schema: schema_path.to_string_lossy().into_owned(),
            include_paths: vec![dir.path().to_path_buf()],
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
            config_file: None,
            outer_file: None,
            theme_on_command_line: false,
            transparent: None,
            radix: None,
        };
        let failure = crate::load::try_load(&options).unwrap_err();
        let schema = crate::load::schema_file(&options);
        let mut app = App::broken(file, &failure, schema.as_deref());
        app.options = Some(options);
        (app, dir)
    }

    #[test]
    fn a_schema_that_does_not_compile_leaves_the_viewer_open_with_the_error_and_the_bytes() {
        let (mut app, _dir) = broken_app(
            "id: bad\nseq:\n  - id: a\n    type: nope\n",
            Some(&[1, 2, 3]),
        );
        let report = app.report.as_ref().unwrap();
        assert!(report.fatal);
        assert_eq!(report.place.as_ref().unwrap().line, 4);
        assert!(
            app.finish_message()
                .unwrap()
                .contains("the schema bad cannot be used")
        );
        // the viewer works: the bytes can be moved in, the keys are handled, q leaves
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Right);
        assert_eq!(app.hex.cursor(), 1);
        key(&mut app, KeyCode::Char('/'));
        key(&mut app, KeyCode::Esc);
        assert!(!app.quit);
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }

    #[test]
    fn a_missing_file_is_a_report_and_the_viewer_has_no_bytes() {
        let (mut app, _dir) = broken_app("id: ok\nseq:\n  - id: a\n    type: u1\n", None);
        assert!(app.report.as_ref().unwrap().fatal);
        assert!(app.bytes.is_none());
        assert!(
            app.finish_message()
                .unwrap()
                .contains("the file cannot be read")
        );
        // every key is safe without a file
        for code in [
            KeyCode::Down,
            KeyCode::Tab,
            KeyCode::Right,
            KeyCode::End,
            KeyCode::Char('i'),
            KeyCode::Char('n'),
        ] {
            key(&mut app, code);
        }
        for text in ["0", "5"] {
            key(&mut app, KeyCode::Char('g'));
            type_text(&mut app, text);
            key(&mut app, KeyCode::Enter);
        }
    }

    /// Rewrite the schema and the file of a viewer made by `app_for` or `broken_app`.
    fn rewrite(dir: &tempfile::TempDir, schema: Option<&str>, data: Option<&[u8]>) {
        if let Some(schema) = schema {
            std::fs::write(dir.path().join("t.yaml"), schema).unwrap();
        }
        if let Some(data) = data {
            std::fs::write(dir.path().join("t.bin"), data).unwrap();
        }
    }

    const V1: &str = "id: rec
seq:
  - id: a
    type: u1
  - id: inner
    type: pair
  - id: c
    type: u1
types:
  pair:
    seq:
      - id: x
        type: u1
      - id: y
        type: u1
";

    /// The same with a field before `inner`, and one inside it.
    const V2: &str = "id: rec
seq:
  - id: new
    type: u1
  - id: a
    type: u1
  - id: inner
    type: pair
  - id: c
    type: u1
types:
  pair:
    seq:
      - id: first
        type: u1
      - id: x
        type: u1
      - id: y
        type: u1
";

    #[test]
    fn after_a_reload_with_a_changed_schema_the_selection_is_on_the_same_field() {
        let (mut app, dir) = app_for(V1, &[1, 2, 3, 4]);
        // open `inner` and select `inner/y`
        app.tree.select_path(&[1, 1]);
        assert_eq!(selected(&app), ["rec", "inner", "y"]);
        assert_eq!(app.tree.selected().summary.as_deref(), Some("3"));
        // the schema gets a field before `inner` and one in it, and the file two bytes more
        rewrite(&dir, Some(V2), Some(&[9, 1, 2, 3, 4, 5]));
        key(&mut app, KeyCode::Char('r'));
        assert_eq!(app.notice.as_deref(), Some("read again"));
        // the same field, which is now at another place in the tree
        assert_eq!(selected(&app), ["rec", "inner", "y"]);
        assert_eq!(app.tree.selected_path(), [2, 2]);
        assert_eq!(app.tree.selected().summary.as_deref(), Some("4"));
        // `inner` is still open
        assert!(app.tree.rows().iter().any(|r| r.name == "first"));
        // the bytes show the new field
        assert_eq!(app.hex.cursor(), 4);
        assert_eq!(app.size, 6);
        assert!(app.report.is_none());
    }

    #[test]
    fn after_a_reload_that_removes_the_field_the_nearest_one_is_selected() {
        let (mut app, dir) = app_for(V1, &[1, 2, 3, 4]);
        app.tree.select_path(&[1, 1]);
        // `inner` has no `y` any more
        let without_y = V1.replace("      - id: y\n        type: u1\n", "");
        rewrite(&dir, Some(&without_y), Some(&[1, 2, 3]));
        key(&mut app, KeyCode::Char('r'));
        assert_eq!(selected(&app), ["rec", "inner"]);
        // and with `inner` gone, the root
        rewrite(
            &dir,
            Some("id: rec\nseq:\n  - id: a\n    type: u1\n"),
            Some(&[1]),
        );
        key(&mut app, KeyCode::Char('r'));
        assert_eq!(selected(&app), ["rec"]);
    }

    #[test]
    fn a_reload_that_fails_keeps_the_old_tree_and_shows_the_error() {
        let (mut app, dir) = app_for(V1, &[1, 2, 3, 4]);
        app.tree.select_path(&[1, 1]);
        let rows_before = app.tree.rows().len();
        rewrite(
            &dir,
            Some("id: rec\nseq:\n  - id: a\n    type: nope\n"),
            None,
        );
        key(&mut app, KeyCode::Char('r'));
        // the old tree is there, with the selection
        assert_eq!(app.tree.rows().len(), rows_before);
        assert_eq!(selected(&app), ["rec", "inner", "y"]);
        let report = app.report.as_ref().unwrap();
        assert!(!report.fatal, "the tree is still shown");
        assert!(
            report
                .headline
                .starts_with("could not read again: the schema rec cannot be used"),
            "{report:?}"
        );
        assert!(report.place.is_some(), "the place in the schema");
        assert!(
            app.notice
                .as_deref()
                .unwrap()
                .starts_with("could not read again")
        );
        assert!(
            app.finish_message().is_some(),
            "the last read failed: the exit says so"
        );
        // the schema is fixed, and r reads it again
        rewrite(&dir, Some(V1), None);
        key(&mut app, KeyCode::Char('r'));
        assert!(app.report.is_none());
        assert_eq!(app.finish_message(), None);
        assert_eq!(selected(&app), ["rec", "inner", "y"]);
    }

    #[test]
    fn a_viewer_that_began_with_an_error_shows_the_tree_after_it_is_fixed() {
        let (mut app, dir) = broken_app("id: rec\nseq:\n  - id: a\n    type: nope\n", None);
        assert!(app.report.as_ref().unwrap().fatal);
        rewrite(
            &dir,
            Some("id: rec\nseq:\n  - id: a\n    type: u1\n"),
            Some(&[7]),
        );
        key(&mut app, KeyCode::Char('r'));
        assert!(app.report.is_none());
        assert!(app.bytes.is_some());
        assert_eq!(app.tree.rows().len(), 2);
        assert_eq!(app.size, 1);
        // and a file that stops being readable shows the error again, still fatal
        std::fs::remove_file(dir.path().join("t.bin")).unwrap();
        key(&mut app, KeyCode::Char('r'));
        assert!(!app.report.as_ref().unwrap().fatal, "the tree is kept");
    }

    #[test]
    fn a_reload_after_the_file_changed_shows_the_new_bytes() {
        let (mut app, dir) = app_for(V1, &[1, 2, 3, 4]);
        app.tree.select_path(&[2]);
        assert_eq!(app.tree.selected().summary.as_deref(), Some("4"));
        rewrite(&dir, None, Some(&[1, 2, 3, 99]));
        key(&mut app, KeyCode::Char('r'));
        assert_eq!(selected(&app), ["rec", "c"]);
        assert_eq!(app.tree.selected().summary.as_deref(), Some("99"));
        // the bytes pane reads the new file
        assert_eq!(app.bytes.as_mut().unwrap().window(3, 1), [99]);
        // a file that now ends too soon: a parse that fails, with the field selected as before
        rewrite(&dir, None, Some(&[1, 2]));
        key(&mut app, KeyCode::Char('r'));
        assert!(app.error.is_some());
        assert!(
            app.report
                .as_ref()
                .unwrap()
                .headline
                .starts_with("the parse failed")
        );
    }

    #[test]
    fn the_scroll_is_kept_as_far_as_the_new_tree_allows() {
        let long =
            "id: big\nseq:\n  - id: items\n    type: u1\n    repeat: expr\n    repeat-expr: 100\n";
        let (mut app, dir) = app_for(long, &[0; 100]);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Right); // open items
        app.tree.select_path(&[0, 90]);
        let theme = crate::ui::Theme::new(false);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 12)).unwrap();
        terminal
            .draw(|f| crate::ui::draw(f, &mut app, &theme))
            .unwrap();
        let top = app.view.top();
        assert!(top > 50, "{top}");
        // the same tree again: the scroll is where it was
        key(&mut app, KeyCode::Char('r'));
        terminal
            .draw(|f| crate::ui::draw(f, &mut app, &theme))
            .unwrap();
        assert_eq!(app.view.top(), top);
        // a tree with 40 elements: the selection falls back to the list, the scroll to what fits
        let shorter = long.replace("100", "40");
        rewrite(&dir, Some(&shorter), Some(&[0; 40]));
        key(&mut app, KeyCode::Char('r'));
        terminal
            .draw(|f| crate::ui::draw(f, &mut app, &theme))
            .unwrap();
        assert_eq!(selected(&app), ["big", "items"]);
        assert!(app.view.top() < app.tree.rows().len(), "{}", app.view.top());
        assert_eq!(
            app.view.top(),
            1,
            "scrolled up just far enough to show the selected list"
        );
    }

    #[test]
    fn imports_are_read_again_and_watched() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("common.yaml"),
            "id: common\nseq:\n  - id: v\n    type: u1\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("t.yaml"),
            "id: top\nimports:\n  - common\nseq:\n  - id: c\n    type: common\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("t.bin"), [5]).unwrap();
        let options = Options {
            file: dir.path().join("t.bin"),
            type_or_schema: dir.path().join("t.yaml").to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
            config_file: None,
            outer_file: None,
            theme_on_command_line: false,
            transparent: None,
            radix: None,
        };
        let mut app = App::new(options.file.clone(), load(&options).unwrap());
        app.options = Some(options);
        // the files that are watched: the schema, its import and the file
        let watched: Vec<String> = app
            .watched_files()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        for name in ["t.yaml", "common.yaml", "t.bin"] {
            assert!(
                watched.iter().any(|w| w == name),
                "{name} is not watched: {watched:?}"
            );
        }
        // the import changes, the schema does not: r shows it
        std::fs::write(
            dir.path().join("common.yaml"),
            "id: common\nseq:\n  - id: v\n    type: u1\n  - id: w\n    type: u1\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("t.bin"), [5, 6]).unwrap();
        key(&mut app, KeyCode::Char('r'));
        app.tree.select_path(&[0]);
        key(&mut app, KeyCode::Right);
        assert!(
            app.tree.rows().iter().any(|r| r.name == "w"),
            "{:?}",
            app.tree.rows().iter().map(|r| &r.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn r_without_anything_to_read_from_says_so_and_a_reload_ends_a_search() {
        let (mut app, _dir) = app_for(V1, &[1, 2, 3, 4]);
        app.options = None;
        key(&mut app, KeyCode::Char('r'));
        assert_eq!(app.notice.as_deref(), Some("nothing to read again from"));
        let (mut app, _dir) = app_for(V1, &[1, 2, 3, 4]);
        app.search = Some(Search::new(Needle::new("x").unwrap(), true, Vec::new()));
        app.reload();
        assert!(!app.searching());
    }

    fn options_in(dir: &tempfile::TempDir, schema: &str, data: Option<&[u8]>) -> Options {
        std::fs::write(dir.path().join("t.yaml"), schema).unwrap();
        if let Some(data) = data {
            std::fs::write(dir.path().join("t.bin"), data).unwrap();
        }
        Options {
            file: dir.path().join("t.bin"),
            type_or_schema: dir.path().join("t.yaml").to_string_lossy().into_owned(),
            include_paths: vec![dir.path().to_path_buf()],
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
            config_file: None,
            outer_file: None,
            theme_on_command_line: false,
            transparent: None,
            radix: None,
        }
    }

    #[test]
    fn a_viewer_that_is_loading_says_so_works_meanwhile_and_takes_the_result() {
        let dir = tempfile::tempdir().unwrap();
        let options = options_in(&dir, V1, Some(&[1, 2, 3, 4]));
        let mut app = App::loading(options.file.clone(), &options.type_or_schema);
        app.options = Some(options.clone());
        assert!(app.loading);
        assert_eq!(
            app.size, 4,
            "the size of the file is known before it is parsed"
        );
        // the keys are safe while it waits; r says that it is busy; q leaves
        for code in [
            KeyCode::Down,
            KeyCode::Right,
            KeyCode::Tab,
            KeyCode::End,
            KeyCode::Char('i'),
            KeyCode::Char('n'),
        ] {
            key(&mut app, code);
        }
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Char('r'));
        assert_eq!(app.notice.as_deref(), Some("still reading the file"));
        // the result comes
        app.finish_loading(crate::load::try_load(&options));
        assert!(!app.loading && app.loading_since.is_none());
        assert!(app.report.is_none());
        assert_eq!(app.type_name, "rec");
        assert!(app.tree.rows().len() >= 4);
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }

    #[test]
    fn a_result_that_is_a_failed_parse_selects_the_field_as_when_it_is_there_at_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let options = options_in(&dir, V1, Some(&[1, 2]));
        let mut app = App::loading(options.file.clone(), &options.type_or_schema);
        app.options = Some(options.clone());
        app.finish_loading(crate::load::try_load(&options));
        assert!(app.error.is_some());
        assert!(app.tree.selected().failed);
        assert!(!app.report.as_ref().unwrap().fatal);
    }

    #[test]
    fn a_result_that_is_an_error_shows_the_report() {
        let dir = tempfile::tempdir().unwrap();
        let options = options_in(&dir, V1, None);
        let mut app = App::loading(options.file.clone(), &options.type_or_schema);
        app.options = Some(options.clone());
        assert_eq!(app.size, 0);
        app.finish_loading(crate::load::try_load(&options));
        assert!(!app.loading);
        assert!(app.report.as_ref().unwrap().fatal);
        assert!(
            app.finish_message()
                .unwrap()
                .contains("the file cannot be read")
        );
        // a thread that ended without an answer is an error too
        let mut app = App::loading(options.file.clone(), "x");
        app.finish_loading(Err(Box::new(crate::load::LoadFailure::stopped(&options))));
        assert!(app.report.as_ref().unwrap().causes[0].contains("stopped without an answer"));
    }

    #[test]
    fn tab_moves_the_keys_between_the_tree_and_the_bytes() {
        let (mut app, _dir) = app();
        assert_eq!(app.focus, Focus::Tree);
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::Hex);
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::Tree);
        key(&mut app, KeyCode::BackTab);
        assert_eq!(app.focus, Focus::Hex);
        assert!(!app.quit);
        // q still leaves from either pane
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }

    #[test]
    fn moving_in_the_bytes_selects_the_innermost_field_of_the_byte() {
        let (mut app, _dir) = app(); // a = byte 0, b = bytes 1..3
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.hex.cursor(), 0);
        key(&mut app, KeyCode::Right);
        assert_eq!(
            (app.hex.cursor(), app.tree.selected().name.as_str()),
            (1, "b")
        );
        key(&mut app, KeyCode::Char('l'));
        assert_eq!(
            (app.hex.cursor(), app.tree.selected().name.as_str()),
            (2, "b")
        );
        key(&mut app, KeyCode::Right); // the end of the file: stays on the last byte
        assert_eq!(app.hex.cursor(), 2);
        key(&mut app, KeyCode::Char('h'));
        key(&mut app, KeyCode::Left);
        assert_eq!(
            (app.hex.cursor(), app.tree.selected().name.as_str()),
            (0, "a")
        );
        key(&mut app, KeyCode::Char('G'));
        assert_eq!(app.hex.cursor(), 2);
        press(&mut app, KeyCode::Home, KeyModifiers::CONTROL);
        assert_eq!(
            (app.hex.cursor(), app.tree.selected().name.as_str()),
            (0, "a")
        );
        // a line down in a file of one line is the last byte; the line keys stay in the file
        key(&mut app, KeyCode::Down);
        assert_eq!(app.hex.cursor(), 2);
        key(&mut app, KeyCode::Up);
        key(&mut app, KeyCode::PageDown);
        key(&mut app, KeyCode::PageUp);
        key(&mut app, KeyCode::End);
        key(&mut app, KeyCode::Home);
        assert!(app.hex.cursor() <= 2);
    }

    #[test]
    fn a_byte_after_what_was_parsed_is_not_in_a_field_and_the_selection_stays() {
        let (mut app, _dir) = app_for("id: t\nseq:\n  - id: a\n    type: u1\n", &[1, 2, 3]);
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.tree.selected().name, "t");
        key(&mut app, KeyCode::Right);
        assert_eq!(app.hex.cursor(), 1);
        assert!(app.notice.as_deref().unwrap().contains("not in a field"));
        // the notice goes with the next key
        key(&mut app, KeyCode::Left);
        assert_eq!(app.hex.cursor(), 0);
        assert!(app.notice.is_none());
    }

    #[test]
    fn moving_in_the_tree_scrolls_the_bytes_to_the_field() {
        // 160 bytes of padding, a marker byte, then a two-byte field
        let schema = "id: t\nseq:\n  - id: pad\n    size: 160\n  - id: marker\n    type: u1\n  - id: tail\n    type: u2le\n";
        let (mut app, _dir) = app_for(schema, &[0; 163]);
        key(&mut app, KeyCode::Down); // pad
        assert_eq!(app.hex.cursor(), 0);
        key(&mut app, KeyCode::Down); // marker
        assert_eq!(app.tree.selected().name, "marker");
        assert_eq!(app.hex.cursor(), 160);
        key(&mut app, KeyCode::Down); // tail
        assert_eq!(app.hex.cursor(), 161);
        key(&mut app, KeyCode::Home); // the root again: the start of the file
        assert_eq!(app.hex.cursor(), 0);
        // going to the bytes and back changes nothing
        key(&mut app, KeyCode::Char('G'));
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.hex.cursor(), 161);
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.tree.selected().name, "tail");
    }

    #[test]
    fn the_keys_move_in_the_tree_and_open_and_close_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let schema = dir.path().join("t.yaml");
        std::fs::write(
            &schema,
            "id: t\nseq:\n  - id: a\n    type: u1\n  - id: inner\n    type: pair\ntypes:\n  pair:\n    seq:\n      - id: x\n        type: u1\n      - id: y\n        type: u1\n",
        )
        .unwrap();
        let file = dir.path().join("t.bin");
        std::fs::write(&file, [1_u8, 2, 3]).unwrap();
        let loaded = crate::load::load(&Options {
            file: file.clone(),
            type_or_schema: schema.to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
            config_file: None,
            outer_file: None,
            theme_on_command_line: false,
            transparent: None,
            radix: None,
        })
        .unwrap();
        let mut app = App::new(file, loaded);
        let name = |app: &App| app.tree.selected().name.clone();

        assert_eq!(name(&app), "t");
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Char('j'));
        assert_eq!(name(&app), "inner");
        key(&mut app, KeyCode::Right); // opens
        assert_eq!(app.tree.rows().len(), 5);
        key(&mut app, KeyCode::Right); // goes into it
        assert_eq!(name(&app), "x");
        key(&mut app, KeyCode::Char('k'));
        key(&mut app, KeyCode::Left); // `inner` is open: closes it
        assert_eq!(app.tree.rows().len(), 3);
        key(&mut app, KeyCode::Left); // closed: the parent
        assert_eq!(name(&app), "t");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.tree.rows().len(), 1, "enter closed the root");
        key(&mut app, KeyCode::Char(' '));
        assert_eq!(app.tree.rows().len(), 3);
        key(&mut app, KeyCode::End);
        assert_eq!(name(&app), "inner");
        key(&mut app, KeyCode::Home);
        assert_eq!(name(&app), "t");
        key(&mut app, KeyCode::Char('G'));
        // the view has not been drawn, so a page is one row
        key(&mut app, KeyCode::PageUp);
        assert_eq!(name(&app), "a");
        key(&mut app, KeyCode::PageDown);
        assert_eq!(name(&app), "inner");
        key(&mut app, KeyCode::Home);
        assert_eq!(name(&app), "t");
        assert!(!app.quit);
    }

    #[test]
    fn other_keys_do_nothing() {
        let (mut app, _dir) = app();
        press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(!app.quit);
    }

    #[test]
    fn the_file_is_parsed() {
        let (app, _dir) = app();
        assert_eq!(app.type_name, "t");
        assert_eq!(app.size, 3);
        assert!(app.error.is_none());
    }
}
