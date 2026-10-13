use doe::parser::cursor::Cursor;

fn cur(data: &[u8]) -> Cursor<'_> {
    Cursor::new(data)
}

// ── Position tracking ─────────────────────────────────────────────────────

#[test]
fn new_cursor_at_zero() {
    let c = cur(&[1, 2, 3]);
    assert_eq!(c.pos(), 0);
    assert_eq!(c.remaining(), 3);
    assert!(!c.is_eof());
}

#[test]
fn eof_on_empty() {
    let c = cur(&[]);
    assert!(c.is_eof());
    assert_eq!(c.remaining(), 0);
}

#[test]
fn pos_advances_after_read() {
    let mut c = cur(&[1, 2, 3, 4]);
    c.read_u8().unwrap();
    assert_eq!(c.pos(), 1);
    assert_eq!(c.remaining(), 3);
}

#[test]
fn absolute_pos_on_root() {
    let mut c = cur(&[1, 2, 3]);
    c.read_u8().unwrap();
    assert_eq!(c.absolute_pos(), 1);
}

#[test]
fn absolute_pos_on_sub_cursor() {
    let buf = [0u8; 10];
    let c = cur(&buf);
    let mut sub = c.sub(4, 3).unwrap();
    sub.read_u8().unwrap();
    assert_eq!(sub.absolute_pos(), 5);
}

// ── Seek ─────────────────────────────────────────────────────────────────

#[test]
fn seek_forward() {
    let mut c = cur(&[1, 2, 3, 4, 5]);
    c.seek(3).unwrap();
    assert_eq!(c.pos(), 3);
    assert_eq!(c.read_u8().unwrap(), 4);
}

#[test]
fn seek_to_end() {
    let mut c = cur(&[1, 2, 3]);
    c.seek(3).unwrap();
    assert!(c.is_eof());
}

#[test]
fn seek_past_end_is_error() {
    let mut c = cur(&[1, 2, 3]);
    assert!(c.seek(4).is_err());
}

// ── read_bytes ────────────────────────────────────────────────────────────

#[test]
fn read_bytes_exact() {
    let mut c = cur(&[0xde, 0xad, 0xbe, 0xef]);
    let b = c.read_bytes(4).unwrap();
    assert_eq!(&*b, &[0xde, 0xad, 0xbe, 0xef]);
    assert!(c.is_eof());
}

#[test]
fn read_bytes_partial_then_rest() {
    let mut c = cur(&[1, 2, 3, 4, 5]);
    assert_eq!(&*c.read_bytes(2).unwrap(), &[1, 2]);
    assert_eq!(&*c.read_bytes(3).unwrap(), &[3, 4, 5]);
}

#[test]
fn read_bytes_overrun_is_error() {
    let mut c = cur(&[1, 2]);
    assert!(c.read_bytes(3).is_err());
}

#[test]
fn read_zero_bytes() {
    let mut c = cur(&[1, 2]);
    assert_eq!(c.read_bytes(0).unwrap(), &[] as &[u8]);
    assert_eq!(c.pos(), 0); // position unchanged
}

// ── peek_bytes ────────────────────────────────────────────────────────────

#[test]
fn peek_does_not_advance() {
    let c = cur(&[1, 2, 3]);
    let p = c.peek_bytes(2).unwrap();
    assert_eq!(&*p, &[1, 2]);
    assert_eq!(c.pos(), 0);
}

#[test]
fn peek_overrun_is_error() {
    let c = cur(&[1]);
    assert!(c.peek_bytes(2).is_err());
}

// ── read_until ────────────────────────────────────────────────────────────

#[test]
fn read_until_terminator() {
    let mut c = cur(b"hello\x00world");
    let s = c.read_until(0x00).unwrap();
    assert_eq!(&*s, b"hello" as &[u8]);
    assert_eq!(c.pos(), 6); // "hello" + null
}

#[test]
fn read_until_at_start() {
    let mut c = cur(&[0x00, 1, 2]);
    let s = c.read_until(0x00).unwrap();
    assert_eq!(s, &[] as &[u8]);
    assert_eq!(c.pos(), 1);
}

#[test]
fn read_until_no_terminator_returns_all() {
    let mut c = cur(b"hello");
    let s = c.read_until(0x00).unwrap();
    assert_eq!(&*s, b"hello" as &[u8]);
    assert!(c.is_eof());
}

#[test]
fn read_until_newline() {
    let mut c = cur(b"line1\nline2\n");
    assert_eq!(&*c.read_until(b'\n').unwrap(), b"line1" as &[u8]);
    assert_eq!(&*c.read_until(b'\n').unwrap(), b"line2" as &[u8]);
    assert!(c.is_eof());
}

// ── Unsigned integer reads ────────────────────────────────────────────────

#[test]
fn read_u8_values() {
    let mut c = cur(&[0x00, 0x7f, 0x80, 0xff]);
    assert_eq!(c.read_u8().unwrap(), 0x00);
    assert_eq!(c.read_u8().unwrap(), 0x7f);
    assert_eq!(c.read_u8().unwrap(), 0x80);
    assert_eq!(c.read_u8().unwrap(), 0xff);
}

#[test]
fn read_u16_le() {
    let mut c = cur(&[0x34, 0x12]);
    assert_eq!(c.read_u16_le().unwrap(), 0x1234);
}

#[test]
fn read_u16_be() {
    let mut c = cur(&[0x12, 0x34]);
    assert_eq!(c.read_u16_be().unwrap(), 0x1234);
}

#[test]
fn read_u32_le() {
    let mut c = cur(&[0x78, 0x56, 0x34, 0x12]);
    assert_eq!(c.read_u32_le().unwrap(), 0x1234_5678);
}

#[test]
fn read_u32_be() {
    let mut c = cur(&[0x12, 0x34, 0x56, 0x78]);
    assert_eq!(c.read_u32_be().unwrap(), 0x1234_5678);
}

#[test]
fn read_u64_le() {
    let data = 0x0102_0304_0506_0708u64.to_le_bytes();
    let mut c = cur(&data);
    assert_eq!(c.read_u64_le().unwrap(), 0x0102_0304_0506_0708);
}

#[test]
fn read_u64_be() {
    let data = 0x0102_0304_0506_0708u64.to_be_bytes();
    let mut c = cur(&data);
    assert_eq!(c.read_u64_be().unwrap(), 0x0102_0304_0506_0708);
}

// ── Signed integer reads ──────────────────────────────────────────────────

#[test]
fn read_i8_positive() {
    let mut c = cur(&[0x7f]);
    assert_eq!(c.read_i8().unwrap(), 127);
}

#[test]
fn read_i8_negative() {
    let mut c = cur(&[0xff]);
    assert_eq!(c.read_i8().unwrap(), -1);
}

#[test]
fn read_i16_le_negative() {
    let mut c = cur(&[0xff, 0xff]);
    assert_eq!(c.read_i16_le().unwrap(), -1);
}

#[test]
fn read_i32_be_negative() {
    let mut c = cur(&[0xff, 0xff, 0xff, 0xff]);
    assert_eq!(c.read_i32_be().unwrap(), -1);
}

#[test]
fn read_i64_le_min() {
    let data = i64::MIN.to_le_bytes();
    let mut c = cur(&data);
    assert_eq!(c.read_i64_le().unwrap(), i64::MIN);
}

// ── Float reads ───────────────────────────────────────────────────────────

#[test]
fn read_f32_le() {
    let v: f32 = 1.5;
    let data = v.to_le_bytes();
    let mut c = cur(&data);
    assert_eq!(c.read_f32_le().unwrap().to_bits(), 1.5f32.to_bits());
}

#[test]
fn read_f32_be() {
    let v: f32 = -2.0;
    let data = v.to_be_bytes();
    let mut c = cur(&data);
    assert_eq!(c.read_f32_be().unwrap().to_bits(), (-2.0f32).to_bits());
}

#[test]
fn read_f64_le() {
    let v: f64 = std::f64::consts::PI;
    let data = v.to_le_bytes();
    let mut c = cur(&data);
    assert!((c.read_f64_le().unwrap() - std::f64::consts::PI).abs() < 1e-15);
}

#[test]
fn read_f64_be() {
    let v: f64 = -1.0;
    let data = v.to_be_bytes();
    let mut c = cur(&data);
    assert_eq!(c.read_f64_be().unwrap().to_bits(), (-1.0f64).to_bits());
}

// ── Sub-cursor ────────────────────────────────────────────────────────────

#[test]
fn sub_cursor_reads_slice() {
    let buf = [0x00, 0x01, 0x02, 0x03, 0x04];
    let c = cur(&buf);
    let mut sub = c.sub(1, 3).unwrap();
    assert_eq!(&*sub.read_bytes(3).unwrap(), &[0x01, 0x02, 0x03]);
    assert!(sub.is_eof());
}

#[test]
fn sub_cursor_out_of_bounds_is_error() {
    let buf = [0u8; 4];
    let c = cur(&buf);
    assert!(c.sub(3, 3).is_err());
}

#[test]
fn sub_cursor_at_end() {
    let buf = [1u8, 2, 3];
    let c = cur(&buf);
    let sub = c.sub(3, 0).unwrap();
    assert!(sub.is_eof());
}

// ── Overrun on typed reads ────────────────────────────────────────────────

#[test]
fn u16_overrun() {
    let mut c = cur(&[0x01]);
    assert!(c.read_u16_le().is_err());
}
#[test]
fn u32_overrun() {
    let mut c = cur(&[0x01, 0x02, 0x03]);
    assert!(c.read_u32_le().is_err());
}
#[test]
fn u64_overrun() {
    let mut c = cur(&[0u8; 7]);
    assert!(c.read_u64_be().is_err());
}
#[test]
fn f32_overrun() {
    let mut c = cur(&[0u8; 3]);
    assert!(c.read_f32_le().is_err());
}
#[test]
fn f64_overrun() {
    let mut c = cur(&[0u8; 7]);
    assert!(c.read_f64_be().is_err());
}

// ── Remaining signed reads ────────────────────────────────────────────────

#[test]
fn read_i16_be_negative() {
    let mut c = cur(&[0xff, 0xfe]);
    assert_eq!(c.read_i16_be().unwrap(), -2);
}

#[test]
fn read_i32_le_negative() {
    let data = (-5i32).to_le_bytes();
    let mut c = cur(&data);
    assert_eq!(c.read_i32_le().unwrap(), -5);
}

#[test]
fn read_i64_be_negative() {
    let data = i64::MIN.to_be_bytes();
    let mut c = cur(&data);
    assert_eq!(c.read_i64_be().unwrap(), i64::MIN);
}

#[test]
fn read_past_end_of_window_is_error_for_every_width() {
    assert!(cur(&[0]).read_i16_be().is_err());
    assert!(cur(&[0; 3]).read_i32_le().is_err());
    assert!(cur(&[0; 7]).read_i64_be().is_err());
}

#[test]
fn read_bytes_with_huge_length_is_error_not_overflow() {
    let mut c = cur(&[1, 2, 3]);
    c.read_u8().unwrap();
    assert!(c.read_bytes(usize::MAX).is_err());
    assert_eq!(c.pos(), 1);
}

#[test]
fn sub_cursor_with_huge_window_is_error_not_overflow() {
    let c = cur(&[1, 2, 3]);
    assert!(c.sub(1, usize::MAX).is_err());
}

// ── File-backed cursors ───────────────────────────────────────────────────

mod file {
    use std::fs::File;

    use doe::DoeError;
    use doe::parser::cursor::{Cursor, MAX_TERMINATED_LEN};
    use tempfile::TempDir;

    /// A file holding `data`; the directory must outlive the cursor.
    fn open(data: &[u8]) -> (TempDir, Cursor<'static>) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("input.bin");
        std::fs::write(&path, data).unwrap();
        let cursor = Cursor::from_file(File::open(&path).unwrap(), &path).unwrap();
        (dir, cursor)
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| u8::try_from((i * 7 + i / 256) % 251).unwrap())
            .collect()
    }

    #[test]
    fn size_and_position() {
        let (_dir, mut c) = open(&[1, 2, 3, 4]);
        assert_eq!(c.len(), 4);
        assert_eq!(c.remaining(), 4);
        assert_eq!(c.read_u16_le().unwrap(), 0x0201);
        assert_eq!(c.pos(), 2);
        assert_eq!(c.absolute_pos(), 2);
        assert_eq!(c.remaining(), 2);
        c.skip(2).unwrap();
        assert!(c.is_eof());
    }

    #[test]
    fn empty_file() {
        let (_dir, mut c) = open(&[]);
        assert!(c.is_empty());
        assert!(c.read_u8().is_err());
        assert!(c.read_until(0).unwrap().is_empty());
    }

    #[test]
    fn reads_match_memory_across_the_cache_boundary() {
        let data = pattern(200_000);
        let (_dir, mut file) = open(&data);
        let mut mem = Cursor::new(&data);
        for at in [
            0, 1, 65_532, 65_533, 65_535, 65_536, 131_071, 131_072, 199_990,
        ] {
            file.seek(at).unwrap();
            mem.seek(at).unwrap();
            assert_eq!(
                file.read_u64_le().unwrap(),
                mem.read_u64_le().unwrap(),
                "at {at}"
            );
            assert_eq!(
                file.read_bytes(2).unwrap(),
                mem.read_bytes(2).unwrap(),
                "at {at}"
            );
        }
    }

    #[test]
    fn large_reads_bypass_the_cache() {
        let data = pattern(300_000);
        let (_dir, mut c) = open(&data);
        assert_eq!(&*c.read_bytes(1).unwrap(), &data[..1]);
        assert_eq!(&*c.read_bytes(200_000).unwrap(), &data[1..200_001]);
        assert_eq!(&*c.read_bytes(99_999).unwrap(), &data[200_001..]);
        assert!(c.is_eof());
    }

    #[test]
    fn peek_does_not_advance() {
        let (_dir, c) = open(&[9, 8, 7]);
        assert_eq!(&*c.peek_bytes(2).unwrap(), &[9, 8][..]);
        assert_eq!(c.pos(), 0);
    }

    #[test]
    fn read_until_crosses_chunks_and_the_cache() {
        let mut data = vec![b'a'; 150_000];
        data.push(0);
        data.extend_from_slice(b"tail");
        let (_dir, mut c) = open(&data);
        assert_eq!(c.read_until(0).unwrap().len(), 150_000);
        assert_eq!(c.pos(), 150_001);
        assert_eq!(&*c.read_until(0).unwrap(), b"tail" as &[u8]);
        assert!(c.is_eof());
    }

    #[test]
    fn read_until_without_a_terminator_returns_the_rest() {
        let (_dir, mut c) = open(b"abc");
        assert_eq!(&*c.read_until(0).unwrap(), b"abc" as &[u8]);
        assert!(c.is_eof());
    }

    #[test]
    fn read_until_is_bounded_for_files_and_memory() {
        let data = vec![b'a'; MAX_TERMINATED_LEN + 1];
        let (_dir, mut file) = open(&data);
        assert!(
            matches!(file.read_until(0), Err(DoeError::Schema(m)) if m.contains("no terminator"))
        );
        let mut mem = Cursor::new(&data);
        assert!(
            matches!(mem.read_until(0), Err(DoeError::Schema(m)) if m.contains("no terminator"))
        );
        // Right at the limit is fine.
        let (_dir, mut ok) = open(&data[..MAX_TERMINATED_LEN]);
        assert_eq!(ok.read_until(0).unwrap().len(), MAX_TERMINATED_LEN);
    }

    #[test]
    fn sub_cursors_read_their_window_of_the_same_file() {
        let data = pattern(200_000);
        let (_dir, c) = open(&data);
        let mut sub = c.sub(100_000, 50).unwrap();
        assert_eq!(sub.len(), 50);
        assert_eq!(sub.absolute_pos(), 100_000);
        assert_eq!(&*sub.read_bytes(50).unwrap(), &data[100_000..100_050]);
        assert!(sub.read_u8().is_err());
        assert!(c.sub(199_999, 2).is_err());
        assert!(c.sub(1, usize::MAX).is_err());
    }

    #[test]
    fn overruns_and_bad_seeks_are_errors() {
        let (_dir, mut c) = open(&[1, 2, 3]);
        assert!(matches!(c.read_bytes(4), Err(DoeError::Schema(m)) if m.contains("overrun")));
        assert!(c.peek_bytes(usize::MAX).is_err());
        assert!(c.skip(4).is_err());
        assert!(c.seek(4).is_err());
        c.seek(3).unwrap();
        assert!(c.read_u8().is_err());
    }

    #[test]
    fn a_file_that_shrinks_is_an_io_error() {
        let data = pattern(100_000);
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("shrinking.bin");
        std::fs::write(&path, &data).unwrap();
        let mut c = Cursor::from_file(File::open(&path).unwrap(), &path).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(10)
            .unwrap();
        c.seek(50_000).unwrap();
        let err = c.read_u32_le().unwrap_err();
        assert!(
            matches!(&err, DoeError::Io { path: p, .. } if *p == path),
            "{err:?}"
        );
    }

    #[test]
    fn behaves_like_memory_for_any_sequence_of_operations() {
        // A small deterministic generator, so a failure can be replayed.
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move |bound: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            usize::try_from(state % u64::try_from(bound).unwrap()).unwrap()
        };
        let mut data = pattern(180_000);
        for i in (0..data.len()).step_by(997) {
            data[i] = 0;
        }
        let (_dir, mut file) = open(&data);
        let mut mem = Cursor::new(&data);
        for step in 0..4000 {
            let op = next(7);
            let (f, m) = match op {
                0 => {
                    let at = next(data.len() + 3);
                    (file.seek(at).map(|()| 0), mem.seek(at).map(|()| 0))
                }
                1 => (file.read_u8().map(u64::from), mem.read_u8().map(u64::from)),
                2 => (
                    file.read_u32_be().map(u64::from),
                    mem.read_u32_be().map(u64::from),
                ),
                3 => (file.read_u64_le(), mem.read_u64_le()),
                4 => {
                    let n = next(70_000);
                    let (f, m) = (file.read_bytes(n), mem.read_bytes(n));
                    (
                        f.map(|b| b.iter().map(|&x| u64::from(x)).sum()),
                        m.map(|b| b.iter().map(|&x| u64::from(x)).sum()),
                    )
                }
                5 => {
                    let f = file.read_until(0).map(|b| u64::try_from(b.len()).unwrap());
                    let m = mem.read_until(0).map(|b| u64::try_from(b.len()).unwrap());
                    (f, m)
                }
                _ => {
                    let n = next(300);
                    (file.skip(n).map(|()| 0), mem.skip(n).map(|()| 0))
                }
            };
            assert_eq!(f.is_ok(), m.is_ok(), "step {step} op {op}");
            if let (Ok(f), Ok(m)) = (f, m) {
                assert_eq!(f, m, "step {step} op {op}");
            }
            assert_eq!(file.pos(), mem.pos(), "step {step} op {op}");
        }
    }
}
