//! Layout shared by generated code and the runtime. Names and offsets only.

/// Bits 0..16 tag, 16..32 display kind, 32..64 payload words excluding the header.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub meta: u64,
    /// 0 white, 1 gray, 2 black. Fresh objects are white.
    pub mark: u32,
    /// Index into [`MapInfo`]. Zero is unwritten, not a layout.
    pub map_id: u32,
}

pub const MARK_WHITE: u32 = 0;
pub const MARK_GRAY: u32 = 1;
pub const MARK_BLACK: u32 = 2;

/// The slot has not been written. Not a layout.
pub const MAP_UNWRITTEN: u32 = 0;
/// Empty `pointer_slots` and `val_is_pointer` clear.
pub const MAP_EMPTY: u32 = 1;

/// Tags, in order, starting at 1.
pub const TAG_CLOSURE: u16 = 1;
pub const TAG_ENV: u16 = 2;
pub const TAG_DELAY: u16 = 3;
pub const TAG_ADAPTED: u16 = 4;
pub const TAG_TAGGED: u16 = 5;
pub const TAG_TUPLE: u16 = 6;
pub const TAG_STRING: u16 = 7;
pub const TAG_KONT: u16 = 8;
pub const TAG_RESUME: u16 = 9;
pub const TAG_CLAUSES: u16 = 10;
pub const TAG_OPERATION: u16 = 11;

/// Display kind in the header flag field. One value, not a slot bitmap.
pub const DISPLAY_CLOSURE: u16 = 1;
pub const DISPLAY_MENU: u16 = 2;
pub const DISPLAY_SELECT: u16 = 3;
pub const DISPLAY_CONSUMER: u16 = 4;
pub const DISPLAY_CONTINUATION: u16 = 5;
pub const DISPLAY_RESUME: u16 = 6;

pub const fn pack_meta(tag: u16, display_kind: u16, payload_words: u32) -> u64 {
    (tag as u64) | ((display_kind as u64) << 16) | ((payload_words as u64) << 32)
}

pub const fn unpack_meta(meta: u64) -> (u16, u16, u32) {
    let tag = meta as u16;
    let display_kind = (meta >> 16) as u16;
    let payload_words = (meta >> 32) as u32;
    (tag, display_kind, payload_words)
}

/// Nine words. Slot 0 begins at byte 72.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub return_address: u64,
    pub cont_prev: u64,
    pub spilled_env: u64,
    pub spilled_handlers: u64,
    pub spilled_val: u64,
    /// Low 32 bits `map_id`, high 32 bits flags.
    pub map_id_and_flags: u64,
    pub frame_words: u64,
    pub prompt_id: u64,
    pub handler_prev: u64,
}

pub const FRAME_RETURN_ADDRESS: usize = 0;
pub const FRAME_CONT_PREV: usize = 8;
pub const FRAME_SPILL_ENV: usize = 16;
pub const FRAME_SPILL_HANDLERS: usize = 24;
pub const FRAME_SPILL_VAL: usize = 32;
pub const FRAME_MAP_FLAGS: usize = 40;
pub const FRAME_FRAME_WORDS: usize = 48;
pub const FRAME_PROMPT_ID: usize = 56;
pub const FRAME_HANDLER_PREV: usize = 64;
pub const FRAME_HEADER_BYTES: usize = 72;
/// Slot 0 is the first word after the header.
pub const FRAME_SLOT0: usize = 72;

/// Bit 0 of the high 32 bits at offset 40.
pub const FRAME_FLAG_PROMPT: u32 = 1;
/// Bit 1. The frame pushed a saved origin, and `ret` pops it.
pub const FRAME_FLAG_ORIGIN: u32 = 2;
/// Bit 2. This frame also replaced the barrier. Bits 8..32 are the id it entered.
pub const FRAME_FLAG_BARRIER: u32 = 4;

pub const fn pack_frame_flags(map_id: u32, flags: u32) -> u64 {
    (map_id as u64) | ((flags as u64) << 32)
}

pub const fn unpack_frame_flags(word: u64) -> (u32, u32) {
    (word as u32, (word >> 32) as u32)
}

/// Closure, menu, co-case, and co-tensor share this prefix. `frame_words` is a `u32`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosurePrefix {
    pub header: Header,
    pub code: u64,
    pub env: u64,
    pub frame_words: u32,
}

pub const CLOSURE_CODE: usize = 16;
pub const CLOSURE_ENV: usize = 24;
pub const CLOSURE_FRAME_WORDS: usize = 32;
/// Generation the closure was built under. A scalar, not a root.
pub const CLOSURE_BIRTH: usize = 48;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaggedPrefix {
    pub header: Header,
    pub label: u32,
    pub payload: u64,
}

pub const TAGGED_LABEL: usize = 16;
pub const TAGGED_PAYLOAD: usize = 24;

/// `bytes` is the first payload byte. The object continues past this prefix.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringPrefix {
    pub header: Header,
    pub byte_len: u64,
    pub char_len: u64,
    pub bytes: u8,
}

pub const STRING_BYTE_LEN: usize = 16;
pub const STRING_CHAR_LEN: usize = 24;
pub const STRING_BYTES: usize = 32;

/// System V register numbers. `rsp` and `rbp` are not SLC registers.
pub const REG_FRAME: u8 = 12;
pub const REG_VAL: u8 = 13;
pub const REG_ENV: u8 = 14;
pub const REG_RETURN_FRAME: u8 = 15;
pub const REG_HANDLERS: u8 = 3;
pub const REG_CALLRT_ARG: u8 = 7;
pub const REG_CALLRT_RESULT: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Safepoint {
    /// Byte offset from the start of [`SLC_TEXT`], not an absolute address.
    pub text_offset: u32,
    pub map_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapInfo {
    pub map_id: u32,
    pub frame_words: u32,
    /// Whether the spilled VAL at offset 32 is a heap pointer.
    pub val_is_pointer: u8,
    /// Slot indices. Slot 0 is the word at offset 72.
    pub pointer_slots: &'static [u16],
}

/// Flat `slc_maps` record. [`MapInfo`] holds a slice, which cannot live in an ELF.
/// `map_id`, `frame_words`, `val_is_pointer`, pad, `slot_count`, then `slot_count` `u16`s.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapRecordHeader {
    pub map_id: u32,
    pub frame_words: u32,
    pub val_is_pointer: u8,
    pub _pad: u8,
    pub slot_count: u16,
}

const _: () = assert!(std::mem::size_of::<MapRecordHeader>() == 12);

pub const SLC_RT_FAIL: &str = "slc_rt_fail";
pub const SLC_RT_FAIL_OVERFLOW: &str = "slc_rt_fail_overflow";
pub const SLC_RT_PROMPT_ANCHOR: &str = "slc_rt_prompt_anchor";
pub const SLC_RT_UNIT: &str = "slc_rt_unit";
pub const SLC_RT_BOOL_TRUE: &str = "slc_rt_bool_true";
pub const SLC_RT_BOOL_FALSE: &str = "slc_rt_bool_false";
pub const SLC_RT_WRAPPING_DIV: &str = "slc_rt_wrapping_div";
pub const SLC_RT_WRAPPING_REM: &str = "slc_rt_wrapping_rem";
pub const SLC_RT_ALLOC: &str = "slc_rt_alloc";
pub const SLC_RT_START: &str = "slc_rt_start";
pub const SLC_RT_ENTER: &str = "slc_rt_enter";
pub const SLC_RT_POLL: &str = "slc_rt_poll";
pub const SLC_RT_STR_CONCAT: &str = "slc_rt_str_concat";
pub const SLC_RT_STR_CMP: &str = "slc_rt_str_cmp";
pub const SLC_RT_STR_EQ: &str = "slc_rt_str_eq";
pub const SLC_RT_TO_I8: &str = "slc_rt_to_i8";
pub const SLC_RT_TO_I32: &str = "slc_rt_to_i32";
pub const SLC_RT_TO_I64: &str = "slc_rt_to_i64";
pub const SLC_RT_TO_U8: &str = "slc_rt_to_u8";
pub const SLC_RT_TO_U32: &str = "slc_rt_to_u32";
pub const SLC_RT_TO_U64: &str = "slc_rt_to_u64";
pub const SLC_RT_INT_TO_STR: &str = "slc_rt_int_to_str";
pub const SLC_RT_FORMAT: &str = "slc_rt_format";
pub const SLC_RT_DISPLAY: &str = "slc_rt_display";
pub const SLC_RT_INDEX: &str = "slc_rt_index";
pub const SLC_RT_SUBSTRING: &str = "slc_rt_substring";
pub const SLC_RT_SKIP_DIGITS: &str = "slc_rt_skip_digits";
pub const SLC_RT_SKIP_WS: &str = "slc_rt_skip_ws";
pub const SLC_RT_CHAR_AT: &str = "slc_rt_char_at";
pub const SLC_RT_FIND_CHAR: &str = "slc_rt_find_char";
pub const SLC_RT_PARSE_INT: &str = "slc_rt_parse_int";
pub const SLC_RT_READ_FILE: &str = "slc_rt_read_file";
pub const SLC_RT_OPEN_FILE: &str = "slc_rt_open_file";
pub const SLC_RT_READ_LINE: &str = "slc_rt_read_line";
pub const SLC_RT_CLOSE_FILE: &str = "slc_rt_close_file";
pub const SLC_RT_WRITE_FILE: &str = "slc_rt_write_file";
pub const SLC_RT_FILE_EXISTS: &str = "slc_rt_file_exists";
pub const SLC_RT_EXIT: &str = "slc_rt_exit";
pub const SLC_RT_FRESH_PROMPT_ID: &str = "slc_rt_fresh_prompt_id";
pub const SLC_RT_STACK_WORDS: &str = "slc_rt_stack_words";
pub const SLC_RT_GC_STATS: &str = "slc_rt_gc_stats";
pub const SLC_PROGRAM_ENTRY: &str = "slc_program_entry";
pub const SLC_FUEL: &str = "slc_fuel";
pub const SLC_C_SP: &str = "slc_c_sp";

pub const SLC_TEXT: &str = "slc_text";
pub const SLC_SAFEPOINTS: &str = "slc_safepoints";
pub const SLC_MAPS: &str = "slc_maps";
pub const SLC_POOL_SCALARS: &str = "slc_pool_scalars";
pub const SLC_POOL_PTRS: &str = "slc_pool_ptrs";
pub const SLC_LABELS: &str = "slc_labels";

pub const START_SLC_TEXT: &str = "__start_slc_text";
pub const STOP_SLC_TEXT: &str = "__stop_slc_text";
pub const START_SLC_SAFEPOINTS: &str = "__start_slc_safepoints";
pub const STOP_SLC_SAFEPOINTS: &str = "__stop_slc_safepoints";
pub const START_SLC_MAPS: &str = "__start_slc_maps";
pub const STOP_SLC_MAPS: &str = "__stop_slc_maps";
pub const START_SLC_POOL_SCALARS: &str = "__start_slc_pool_scalars";
pub const STOP_SLC_POOL_SCALARS: &str = "__stop_slc_pool_scalars";
pub const START_SLC_POOL_PTRS: &str = "__start_slc_pool_ptrs";
pub const STOP_SLC_POOL_PTRS: &str = "__stop_slc_pool_ptrs";
pub const START_SLC_LABELS: &str = "__start_slc_labels";
pub const STOP_SLC_LABELS: &str = "__stop_slc_labels";

/// Linker-facing names. Not declarations of the functions.
pub const EXPORTED_SYMBOLS: &[&str] = &[
    SLC_RT_FAIL,
    SLC_RT_FAIL_OVERFLOW,
    SLC_RT_PROMPT_ANCHOR,
    SLC_RT_UNIT,
    SLC_RT_BOOL_TRUE,
    SLC_RT_BOOL_FALSE,
    SLC_RT_WRAPPING_DIV,
    SLC_RT_WRAPPING_REM,
    SLC_RT_ALLOC,
    SLC_RT_START,
    SLC_RT_ENTER,
    SLC_RT_POLL,
    SLC_RT_STR_CONCAT,
    SLC_RT_STR_CMP,
    SLC_RT_STR_EQ,
    SLC_RT_TO_I8,
    SLC_RT_TO_I32,
    SLC_RT_TO_I64,
    SLC_RT_TO_U8,
    SLC_RT_TO_U32,
    SLC_RT_TO_U64,
    SLC_RT_INT_TO_STR,
    SLC_RT_FORMAT,
    SLC_RT_DISPLAY,
    SLC_RT_INDEX,
    SLC_RT_SUBSTRING,
    SLC_RT_SKIP_DIGITS,
    SLC_RT_SKIP_WS,
    SLC_RT_CHAR_AT,
    SLC_RT_FIND_CHAR,
    SLC_RT_PARSE_INT,
    SLC_RT_READ_FILE,
    SLC_RT_OPEN_FILE,
    SLC_RT_READ_LINE,
    SLC_RT_CLOSE_FILE,
    SLC_RT_WRITE_FILE,
    SLC_RT_FILE_EXISTS,
    SLC_RT_EXIT,
    SLC_RT_FRESH_PROMPT_ID,
    SLC_RT_STACK_WORDS,
    SLC_RT_GC_STATS,
    SLC_PROGRAM_ENTRY,
    SLC_FUEL,
    SLC_C_SP,
    SLC_TEXT,
    SLC_SAFEPOINTS,
    SLC_MAPS,
    SLC_POOL_SCALARS,
    SLC_POOL_PTRS,
    SLC_LABELS,
    START_SLC_TEXT,
    STOP_SLC_TEXT,
    START_SLC_SAFEPOINTS,
    STOP_SLC_SAFEPOINTS,
    START_SLC_MAPS,
    STOP_SLC_MAPS,
    START_SLC_POOL_SCALARS,
    STOP_SLC_POOL_SCALARS,
    START_SLC_POOL_PTRS,
    STOP_SLC_POOL_PTRS,
    START_SLC_LABELS,
    STOP_SLC_LABELS,
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn header_frame_and_object_offsets() {
        assert_eq!(size_of::<Header>(), 16);
        assert_eq!(offset_of!(Header, meta), 0);
        assert_eq!(offset_of!(Header, mark), 8);
        assert_eq!(offset_of!(Header, map_id), 12);
        assert_eq!(size_of::<FrameHeader>(), 72);
        assert_eq!(FRAME_HEADER_BYTES, 72);
        assert_eq!(FRAME_SLOT0, 72);
        assert_eq!(size_of::<FrameHeader>(), FRAME_SLOT0);
        assert_eq!(offset_of!(FrameHeader, return_address), FRAME_RETURN_ADDRESS);
        assert_eq!(offset_of!(FrameHeader, cont_prev), 8);
        assert_eq!(offset_of!(FrameHeader, spilled_env), 16);
        assert_eq!(offset_of!(FrameHeader, spilled_handlers), 24);
        assert_eq!(offset_of!(FrameHeader, spilled_val), 32);
        assert_eq!(offset_of!(FrameHeader, map_id_and_flags), 40);
        assert_eq!(offset_of!(FrameHeader, frame_words), 48);
        assert_eq!(offset_of!(FrameHeader, prompt_id), 56);
        assert_eq!(offset_of!(FrameHeader, handler_prev), 64);
        assert_eq!(offset_of!(ClosurePrefix, code), CLOSURE_CODE);
        assert_eq!(offset_of!(ClosurePrefix, env), CLOSURE_ENV);
        assert_eq!(offset_of!(ClosurePrefix, frame_words), 32);
        assert_eq!(CLOSURE_FRAME_WORDS, 32);
        assert_eq!(CLOSURE_BIRTH, 48);
        assert_eq!(FRAME_FLAG_ORIGIN, 2);
        assert_eq!(FRAME_FLAG_BARRIER, 4);
        assert_eq!(offset_of!(TaggedPrefix, label), 16);
        assert_eq!(offset_of!(TaggedPrefix, payload), 24);
        assert_eq!(TAGGED_LABEL, 16);
        assert_eq!(TAGGED_PAYLOAD, 24);
        assert_eq!(offset_of!(StringPrefix, byte_len), 16);
        assert_eq!(offset_of!(StringPrefix, char_len), 24);
        assert_eq!(offset_of!(StringPrefix, bytes), 32);
        assert_eq!(MAP_EMPTY, 1);
        assert_eq!(MAP_UNWRITTEN, 0);
        assert_eq!((MARK_WHITE, MARK_GRAY, MARK_BLACK), (0, 1, 2));
        assert_eq!(
            [
                TAG_CLOSURE,
                TAG_ENV,
                TAG_DELAY,
                TAG_ADAPTED,
                TAG_TAGGED,
                TAG_TUPLE,
                TAG_STRING,
                TAG_KONT,
                TAG_RESUME,
                TAG_CLAUSES,
                TAG_OPERATION,
            ],
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
        );
        assert_eq!(FRAME_FLAG_PROMPT, 1);
        assert_eq!(
            (
                REG_FRAME,
                REG_VAL,
                REG_ENV,
                REG_RETURN_FRAME,
                REG_HANDLERS,
                REG_CALLRT_ARG,
                REG_CALLRT_RESULT,
            ),
            (12, 13, 14, 15, 3, 7, 0)
        );
    }

    #[test]
    fn meta_and_frame_flags_use_one_shift() {
        let meta = pack_meta(TAG_STRING, 0, 4);
        assert_eq!(unpack_meta(meta), (TAG_STRING, 0, 4));
        assert_eq!(meta >> 32, 4);
        let word = pack_frame_flags(MAP_EMPTY, FRAME_FLAG_PROMPT);
        assert_eq!(unpack_frame_flags(word), (MAP_EMPTY, FRAME_FLAG_PROMPT));
        assert_eq!(word as u32, MAP_EMPTY);
    }

    #[test]
    fn exported_symbols_omit_interpreter_mechanisms() {
        for forbidden in ["slc_rt_add", "__match_dispatch", "__handle"] {
            assert!(
                !EXPORTED_SYMBOLS.contains(&forbidden),
                "{forbidden} is not an exported symbol"
            );
        }
    }

    #[test]
    fn each_section_has_a_gnu_bounds_pair() {
        for section in
            [SLC_TEXT, SLC_SAFEPOINTS, SLC_MAPS, SLC_POOL_SCALARS, SLC_POOL_PTRS, SLC_LABELS]
        {
            let start = format!("__start_{section}");
            let stop = format!("__stop_{section}");
            assert!(EXPORTED_SYMBOLS.contains(&start.as_str()));
            assert!(EXPORTED_SYMBOLS.contains(&stop.as_str()));
        }
    }
}
