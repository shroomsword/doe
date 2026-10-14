//! Drives `Cursor` with an arbitrary sequence of operations over arbitrary bytes.
//! Every operation must return `Ok` or `Err`; none may panic or overflow.

#![no_main]

use arbitrary::Arbitrary;
use doe::parser::cursor::{BitOrder, Cursor};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum Op {
    ReadBytes(usize),
    PeekBytes(usize),
    ReadUntil(u8),
    ReadBits(u8, bool),
    Mark,
    Reset,
    Skip(usize),
    Seek(usize),
    Sub(usize, usize),
    U8,
    I8,
    U16Le,
    U16Be,
    U32Le,
    U32Be,
    U64Le,
    U64Be,
    I16Le,
    I16Be,
    I32Le,
    I32Be,
    I64Le,
    I64Be,
    F32Le,
    F32Be,
    F64Le,
    F64Be,
}

fn apply(cursor: &mut Cursor<'_>, op: &Op, mark: &mut Option<doe::parser::cursor::Mark>) {
    match *op {
        Op::ReadBits(bits, lsb) => {
            let order = if lsb {
                BitOrder::LeastSignificantFirst
            } else {
                BitOrder::MostSignificantFirst
            };
            let before = cursor.bit_position();
            if let Ok(value) = cursor.read_bits(bits, order) {
                assert!(u32::from(bits) == 64 || value < 1u64 << bits);
                // Switching the bit order mid-byte also drops the unread bits.
                assert!(cursor.bit_position() >= before + u64::from(bits));
            }
        }
        Op::Mark => *mark = Some(cursor.mark()),
        Op::Reset => {
            if let Some(mark) = *mark {
                cursor.reset(mark);
            }
        }
        Op::Skip(n) => drop(cursor.skip(n)),
        Op::ReadBytes(n) => drop(cursor.read_bytes(n)),
        Op::PeekBytes(n) => drop(cursor.peek_bytes(n)),
        Op::ReadUntil(b) => drop(cursor.read_until(b)),
        Op::Seek(pos) => drop(cursor.seek(pos)),
        Op::Sub(start, len) => {
            if let Ok(mut sub) = cursor.sub(start, len) {
                drop(sub.read_u8());
                assert!(sub.absolute_pos() >= start);
            }
        }
        Op::U8 => drop(cursor.read_u8()),
        Op::I8 => drop(cursor.read_i8()),
        Op::U16Le => drop(cursor.read_u16_le()),
        Op::U16Be => drop(cursor.read_u16_be()),
        Op::U32Le => drop(cursor.read_u32_le()),
        Op::U32Be => drop(cursor.read_u32_be()),
        Op::U64Le => drop(cursor.read_u64_le()),
        Op::U64Be => drop(cursor.read_u64_be()),
        Op::I16Le => drop(cursor.read_i16_le()),
        Op::I16Be => drop(cursor.read_i16_be()),
        Op::I32Le => drop(cursor.read_i32_le()),
        Op::I32Be => drop(cursor.read_i32_be()),
        Op::I64Le => drop(cursor.read_i64_le()),
        Op::I64Be => drop(cursor.read_i64_be()),
        Op::F32Le => drop(cursor.read_f32_le()),
        Op::F32Be => drop(cursor.read_f32_be()),
        Op::F64Le => drop(cursor.read_f64_le()),
        Op::F64Be => drop(cursor.read_f64_be()),
    }
}

fuzz_target!(|input: (Vec<u8>, Vec<Op>)| {
    let (data, ops) = input;
    let mut cursor = Cursor::new(&data);
    let mut mark = None;
    for op in &ops {
        apply(&mut cursor, op, &mut mark);
        // The position never leaves the window.
        assert!(cursor.pos() <= data.len());
        assert_eq!(cursor.remaining(), data.len() - cursor.pos());
    }
});
