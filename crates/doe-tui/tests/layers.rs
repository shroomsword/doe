//! A type that holds another file, chosen in the viewer, is shown as what it holds: `gzip_file` on a `.tar.gz` is the tar.

use std::path::PathBuf;

use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;

fn types() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../types")
}

fn options(file: PathBuf, type_or_schema: &str) -> Options {
    Options {
        file,
        type_or_schema: type_or_schema.to_owned(),
        include_paths: vec![types()],
        color: true,
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
fn choosing_a_type_that_holds_a_file_shows_the_file_it_holds() {
    let file = types().join("samples/sample.tar.gz");
    let options = options(file.clone(), "gzip_file");
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    assert_eq!(app.type_name, "gzip_file");

    app.choose_available("gzip_file");
    assert_eq!(app.type_name, "tar");
    assert_eq!(app.file.file_name().unwrap(), "sample.tar");
    assert_eq!(
        std::fs::read(&app.file).unwrap(),
        std::fs::read(types().join("samples/sample.tar")).unwrap()
    );
    let layer = app.file.clone();
    assert_eq!(
        app.options.as_ref().unwrap().outer_file.as_deref(),
        Some(file.as_path())
    );

    // a type that holds nothing leaves the layer that is shown
    app.choose_available("tar");
    assert_eq!(app.type_name, "tar");
    assert_eq!(app.file, layer);
}
