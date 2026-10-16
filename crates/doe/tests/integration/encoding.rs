use doe::DoeError;
use doe::parser::encoding::decode;
use doe::schema::ir::Encoding;

fn text(bytes: &[u8], encoding: Encoding) -> String {
    decode(bytes, encoding).unwrap()
}

fn message(bytes: &[u8], encoding: Encoding) -> String {
    match decode(bytes, encoding).unwrap_err() {
        DoeError::Schema(message) => message,
        other => panic!("{other:?}"),
    }
}

#[test]
fn utf16_in_both_byte_orders_with_a_pair_of_surrogates() {
    assert_eq!(text(b"h\0i\0", Encoding::Utf16Le), "hi");
    assert_eq!(text(b"\0h\0i", Encoding::Utf16Be), "hi");
    assert_eq!(
        text(&[0x3d, 0xd8, 0x00, 0xde], Encoding::Utf16Le),
        "\u{1f600}"
    );
    assert_eq!(
        text(&[0xd8, 0x3d, 0xde, 0x00], Encoding::Utf16Be),
        "\u{1f600}"
    );
    assert_eq!(text(b"", Encoding::Utf16Le), "");
}

#[test]
fn utf16_errors_name_the_offset() {
    assert!(message(b"h\0i", Encoding::Utf16Le).contains("byte 2"));
    // A lone high surrogate after one good unit.
    let m = message(&[0x68, 0x00, 0x3d, 0xd8], Encoding::Utf16Le);
    assert!(m.contains("UTF-16LE") && m.contains("byte 2"), "{m}");
}

#[test]
fn utf32_in_both_byte_orders() {
    assert_eq!(
        text(&[0x00, 0xf6, 0x01, 0x00], Encoding::Utf32Le),
        "\u{1f600}"
    );
    assert_eq!(
        text(&[0x00, 0x01, 0xf6, 0x00], Encoding::Utf32Be),
        "\u{1f600}"
    );
    assert!(message(&[0, 0, 0], Encoding::Utf32Le).contains("UTF-32LE"));
    let m = message(&[0x41, 0, 0, 0, 0x00, 0x00, 0x11, 0x00], Encoding::Utf32Le);
    assert!(m.contains("byte 4"), "{m}");
}

#[test]
fn cp437_maps_the_high_half() {
    assert_eq!(
        text(&[0x41, 0x80, 0xb0, 0xe1, 0xfe, 0xff], Encoding::Cp437),
        "A\u{c7}\u{2591}\u{df}\u{25a0}\u{a0}"
    );
}

#[test]
fn legacy_encodings_decode_and_reject_malformed_input() {
    assert_eq!(
        text(&[0x80, 0x93], Encoding::Legacy("windows-1252")),
        "\u{20ac}\u{201c}"
    );
    assert_eq!(text(&[0xa1], Encoding::Legacy("ISO-8859-2")), "\u{104}");
    assert_eq!(
        text(&[0x82, 0xa0], Encoding::Legacy("Shift_JIS")),
        "\u{3042}"
    );
    assert_eq!(text(&[0xc4, 0xe3], Encoding::Legacy("GBK")), "\u{4f60}");
    assert_eq!(text(&[0xa4, 0x40], Encoding::Legacy("Big5")), "\u{4e00}");
    assert_eq!(text(&[0xa4, 0xa2], Encoding::Legacy("EUC-JP")), "\u{3042}");
    assert_eq!(text(&[0xb0, 0xa1], Encoding::Legacy("EUC-KR")), "\u{ac00}");
    let m = message(&[0x41, 0x82], Encoding::Legacy("Shift_JIS"));
    assert!(m.contains("Shift_JIS") && m.contains("byte 1"), "{m}");
}

#[test]
fn ascii_errors_name_the_offset_and_the_old_encodings_still_work() {
    assert!(message(b"ab\xff", Encoding::Ascii).contains("byte 2"));
    assert_eq!(text(&[0xe9], Encoding::Latin1), "\u{e9}");
    assert!(decode(&[0xff], Encoding::Utf8).is_err());
}
