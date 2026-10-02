//! Stack, heap, and mark-sweep for native SLC. Frames are built by hand; nothing here compiles.

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

use slc_abi::{
    DISPLAY_CLOSURE, DISPLAY_CONSUMER, DISPLAY_CONTINUATION, DISPLAY_MENU, DISPLAY_RESUME,
    DISPLAY_SELECT, FRAME_CONT_PREV, FRAME_FLAG_BARRIER, FRAME_FLAG_ORIGIN, FRAME_FLAG_PROMPT,
    FRAME_FRAME_WORDS, FRAME_HANDLER_PREV, FRAME_HEADER_BYTES, FRAME_MAP_FLAGS, FRAME_PROMPT_ID,
    FRAME_SLOT0, FRAME_SPILL_ENV, FRAME_SPILL_HANDLERS, FRAME_SPILL_VAL, FrameHeader, Header,
    MAP_EMPTY, MAP_UNWRITTEN, MARK_BLACK, MARK_WHITE, STRING_BYTE_LEN, STRING_BYTES,
    STRING_CHAR_LEN, TAG_ADAPTED, TAG_CLAUSES, TAG_CLOSURE, TAG_DELAY, TAG_ENV, TAG_KONT,
    TAG_OPERATION, TAG_RESUME, TAG_STRING, TAG_TAGGED, TAG_TUPLE, TAGGED_LABEL, TAGGED_PAYLOAD,
    pack_frame_flags, pack_meta, unpack_frame_flags, unpack_meta,
};

const INITIAL_SEGMENT_BYTES: usize = 1 << 20;
const MAX_SEGMENT_BYTES: usize = 1 << 30;
/// Fresh objects are carved from a chunk. A swept object leaves a hole; the
/// chunk is freed only when the runtime drops. Individual bump addresses are
/// not passed to `dealloc`.
const CHUNK_BYTES: usize = 1 << 20;
const MAX_CHUNKS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtError {
    StackOverflow,
    ForeignPrompt,
    /// No installed prompt named this operation id.
    Unhandled(u64),
}

impl std::fmt::Display for RtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RtError::StackOverflow => f.write_str("SLC stack overflow"),
            RtError::ForeignPrompt => f.write_str(
                "a continuation left the handler it was captured under: it was jumped to under another",
            ),
            RtError::Unhandled(id) => {
                write!(f, "error: type mismatch: no handler for operation `{}`", label_named(*id))
            }
        }
    }
}

impl std::error::Error for RtError {}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GcStats {
    pub collections: u64,
    pub bytes_swept: u64,
    pub objects_swept: u64,
}

#[derive(Clone)]
struct MapRecord {
    val_is_pointer: bool,
    pointer_slots: Vec<u16>,
}

struct Chunk {
    ptr: *mut u8,
    len: usize,
    /// Bytes carved from the front of the chunk. The tail stays unused.
    used: usize,
    layout: Layout,
    /// One bit per 16-byte slot. The base bit is stored after the header write.
    /// Allocated at its final size so the published pointer stays valid when
    /// this chunk moves inside `Vec`.
    bitmap: Box<[AtomicU64]>,
    /// Byte size of the object whose base is this slot. Zero is empty.
    /// Interior slots stay zero. A live size is 16-aligned and non-zero.
    slots: Box<[u32]>,
}

enum CaptureStop {
    /// Through the outermost prompt.
    Outermost,
    /// Through the nearest prompt. This is the slice a `Resume` reinstates.
    Nearest,
    /// Through this live frame, inclusive. Perform copies nearer prompts too.
    Through(*const u8),
}

pub struct Runtime {
    /// Replaced wholesale on growth so interior pointers stay valid until that copy.
    mem: Vec<u8>,
    segment_cap: usize,
    sp_off: Option<usize>,
    maps: HashMap<u32, MapRecord>,
    chunks: Vec<Chunk>,
    /// Chunk that still has room, and the next free offset in it.
    bump_index: usize,
    bump_off: usize,
    /// The process runtime publishes chunks so `slc_rt_word_tag` can run
    /// without the runtime lock. Test runtimes leave this off.
    publishing: bool,
    immortal: Vec<u64>,
    global_table: Vec<u64>,
    pointer_pool: Vec<u64>,
    next_prompt: u64,
    watermark: usize,
    bytes_since_gc: usize,
    stats: GcStats,
    unit: u64,
    entered_code: u64,
    /// Live `slc_pool_ptrs` words. Scanned in place; a startup copy would miss later stores.
    pool_section: *const u64,
    pool_section_len: usize,
}

// Owned addresses. The process entry shares one runtime under a mutex.
unsafe impl Send for Runtime {}

impl Drop for Runtime {
    fn drop(&mut self) {
        if self.publishing {
            PUBLISHED.spans.store(std::ptr::null_mut(), Ordering::Release);
            slc_bump.store(0, Ordering::Release);
            slc_bump_end.store(0, Ordering::Release);
        }
        for chunk in self.chunks.drain(..) {
            unsafe { dealloc(chunk.ptr, chunk.layout) }
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        let requested = slc_segment_bytes.load(Ordering::Relaxed) as usize;
        Self::with_segment_bytes(if requested == 0 { INITIAL_SEGMENT_BYTES } else { requested })
    }

    pub fn with_segment_bytes(n: usize) -> Self {
        assert!(n > 0);
        let mut rt = Self {
            mem: vec![0u8; n],
            segment_cap: MAX_SEGMENT_BYTES,
            sp_off: None,
            maps: HashMap::new(),
            chunks: Vec::new(),
            bump_index: 0,
            bump_off: 0,
            publishing: false,
            immortal: Vec::new(),
            global_table: Vec::new(),
            pointer_pool: Vec::new(),
            next_prompt: 1,
            watermark: usize::MAX,
            bytes_since_gc: 0,
            stats: GcStats::default(),
            unit: 0,
            entered_code: 0,
            pool_section: std::ptr::null(),
            pool_section_len: 0,
        };
        let unit = rt.alloc(2, TAG_TAGGED, MAP_EMPTY);
        rt.unit = unit as u64;
        rt.immortal.push(rt.unit);
        slc_rt_unit.store(rt.unit, Ordering::Relaxed);
        rt.publish_limit();
        rt
    }

    pub fn set_segment_cap(&mut self, bytes: usize) {
        self.segment_cap = bytes;
    }

    pub fn set_alloc_watermark(&mut self, bytes: usize) {
        self.watermark = bytes;
    }

    pub fn segment_base(&self) -> *const u8 {
        self.mem.as_ptr()
    }

    pub fn segment_bytes(&self) -> Vec<u8> {
        self.mem.clone()
    }

    pub fn sp(&self) -> *mut u8 {
        match self.sp_off {
            Some(off) => self.ptr_at(off),
            None => std::ptr::null_mut(),
        }
    }

    pub fn set_sp(&mut self, frame: *mut u8) {
        assert!(self.in_segment(frame as u64), "sp is not in the segment");
        self.sp_off = Some(frame as usize - self.mem.as_ptr() as usize);
    }

    pub fn read(&self, ptr: *const u8, offset: usize) -> u64 {
        read_u64(ptr, offset)
    }

    pub fn write(&mut self, ptr: *mut u8, offset: usize, value: u64) {
        write_u64(ptr, offset, value);
    }

    pub fn unit(&self) -> *const u8 {
        self.unit as *const u8
    }

    pub fn entered_code(&self) -> u64 {
        self.entered_code
    }

    pub fn fresh_prompt_id(&mut self) -> u64 {
        let id = self.next_prompt;
        self.next_prompt += 1;
        id
    }

    pub fn stack_words(&self) -> u64 {
        self.live_frames().iter().map(|frame| self.read(*frame, FRAME_FRAME_WORDS)).sum()
    }

    pub fn gc_stats(&self) -> GcStats {
        self.stats
    }

    pub fn register_map(&mut self, map_id: u32, val_is_pointer: bool, pointer_slots: &[u16]) {
        assert!(map_id != MAP_UNWRITTEN, "id 0 is not a layout");
        if map_id == MAP_EMPTY {
            return;
        }
        self.maps
            .insert(map_id, MapRecord { val_is_pointer, pointer_slots: pointer_slots.to_vec() });
    }

    pub fn install_globals(&mut self, words: &[u64]) {
        self.global_table = words.to_vec();
    }

    pub fn install_pointer_pool(&mut self, words: &[u64]) {
        self.pointer_pool = words.to_vec();
    }

    pub fn is_live(&self, ptr: *const u8) -> bool {
        self.slot_size(ptr as u64).is_some()
    }

    pub fn object_size(&self, ptr: *const u8) -> usize {
        self.slot_size(ptr as u64).expect("live object")
    }

    pub fn object_bytes(&self, ptr: *const u8) -> &[u8] {
        let size = self.object_size(ptr);
        unsafe { std::slice::from_raw_parts(ptr, size) }
    }

    pub fn object_header(&self, ptr: *const u8) -> Header {
        unsafe { std::ptr::read(ptr as *const Header) }
    }

    pub fn push_frame(&mut self, frame_words: u32) -> Result<(), RtError> {
        assert!(frame_words >= 9, "frame_words counts the header");
        let bytes = frame_words as usize * 8;
        let off = match self.sp_off {
            Some(sp) => sp + self.frame_nbytes_off(sp),
            None => 0,
        };
        self.ensure(off + bytes)?;
        let cont_prev = match self.sp_off {
            Some(sp) => self.ptr_at(sp) as u64,
            None => anchor_addr(),
        };
        let dst = self.ptr_at(off);
        unsafe { std::ptr::write_bytes(dst, 0, bytes) }
        write_u64(dst, FRAME_CONT_PREV, cont_prev);
        write_u64(dst, FRAME_FRAME_WORDS, u64::from(frame_words));
        write_u64(dst, FRAME_MAP_FLAGS, pack_frame_flags(MAP_EMPTY, 0));
        self.sp_off = Some(off);
        Ok(())
    }

    pub fn push_prompt(&mut self, id: u64, frame_words: u32) -> Result<(), RtError> {
        assert!(id != 0, "prompt ids start at 1");
        self.push_frame(frame_words)?;
        let frame = self.sp();
        let (map, _) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
        self.write(frame, FRAME_MAP_FLAGS, pack_frame_flags(map, FRAME_FLAG_PROMPT));
        self.stamp_barrier(frame);
        self.write(frame, FRAME_PROMPT_ID, id);
        let below = self.read(frame, FRAME_CONT_PREV) as *const u8;
        self.write(frame, FRAME_HANDLER_PREV, self.prompt_from(below));
        Ok(())
    }

    /// Caller-restore slots (return address, `cont_prev`) stay as they are.
    pub fn safepoint_spill(&mut self, env: u64, handlers: u64, val: u64, map_id: u32, flags: u32) {
        let frame = self.sp();
        self.write(frame, FRAME_SPILL_ENV, env);
        self.write(frame, FRAME_SPILL_HANDLERS, handlers);
        self.write(frame, FRAME_SPILL_VAL, val);
        self.write(frame, FRAME_MAP_FLAGS, pack_frame_flags(map_id, flags));
    }

    /// Keeps the frame base, the return address, and `cont_prev`.
    pub fn tail_slide(
        &mut self,
        env: u64,
        slots: &[u64],
        map_id: u32,
        frame_words: u32,
    ) -> Result<(), RtError> {
        let sp = self.sp_off.expect("tail slide needs a frame");
        let bytes = frame_words as usize * 8;
        assert!(FRAME_SLOT0 + slots.len() * 8 <= bytes, "slots do not fit in frame_words");
        // Grow first so a failed growth leaves the old segment untouched.
        self.ensure(sp + bytes)?;
        let frame = self.ptr_at(sp);
        let old_bytes = read_u64(frame, FRAME_FRAME_WORDS) as usize * 8;
        if bytes > old_bytes {
            unsafe { std::ptr::write_bytes(frame.add(old_bytes), 0, bytes - old_bytes) }
        }
        let (_, flags) = unpack_frame_flags(read_u64(frame, FRAME_MAP_FLAGS));
        write_u64(frame, FRAME_SPILL_ENV, env);
        write_u64(frame, FRAME_MAP_FLAGS, pack_frame_flags(map_id, flags));
        write_u64(frame, FRAME_FRAME_WORDS, u64::from(frame_words));
        for (i, slot) in slots.iter().enumerate() {
            write_u64(frame, FRAME_SLOT0 + i * 8, *slot);
        }
        Ok(())
    }

    pub fn alloc(&mut self, words: u32, tag: u16, map_id: u32) -> *mut u8 {
        let display = match tag {
            TAG_KONT => DISPLAY_CONTINUATION,
            TAG_RESUME => DISPLAY_RESUME,
            TAG_CLOSURE => DISPLAY_CLOSURE,
            _ => 0,
        };
        self.alloc_raw(16 + words as usize * 8, tag, display, words, map_id)
    }

    pub fn alloc_delay(&mut self, code: u64, env: u64) -> *mut u8 {
        let delay = self.alloc(2, TAG_DELAY, MAP_EMPTY);
        self.write(delay, 16, code);
        self.write(delay, 24, env);
        delay
    }

    /// `ENV` comes from the delay. `HANDLERS` stays the caller's. Nothing is stored back.
    pub fn enter_delay(&mut self, delay: *const u8) {
        let code = self.read(delay, 16);
        let env = self.read(delay, 24);
        let frame = self.sp();
        self.write(frame, FRAME_SPILL_ENV, env);
        self.write(frame, FRAME_SPILL_VAL, self.unit);
        self.entered_code = code;
    }

    pub fn capture(&mut self) -> *mut u8 {
        self.capture_image(CaptureStop::Outermost, TAG_KONT, DISPLAY_CONTINUATION)
    }

    pub fn capture_resume(&mut self) -> *mut u8 {
        self.capture_image(CaptureStop::Nearest, TAG_RESUME, DISPLAY_RESUME)
    }

    /// Copy through `frame`, which is the prompt whose clauses answered.
    pub fn capture_through(&mut self, frame: *const u8) -> *mut u8 {
        self.capture_image(CaptureStop::Through(frame), TAG_RESUME, DISPLAY_RESUME)
    }

    pub fn invoke(&mut self, image: *const u8) -> Result<(), RtError> {
        let prompt = self.live_frames().into_iter().find(|frame| self.is_prompt(*frame));
        let Some(prompt) = prompt else {
            return self.install_replace(image);
        };
        let id = self.read(prompt, FRAME_PROMPT_ID);
        let prompt_off = self.offset_of(prompt);
        let image_frames = self.image_frames(image);
        // The first live prompt decides. An unknown id must not fall through to an outer one.
        let Some(heap_prompt) = image_frames
            .iter()
            .copied()
            .find(|frame| self.is_prompt(*frame) && self.read(*frame, FRAME_PROMPT_ID) == id)
        else {
            return Err(RtError::ForeignPrompt);
        };
        let above: Vec<*const u8> =
            image_frames.into_iter().take_while(|frame| *frame != heap_prompt).collect();
        self.install_above(prompt_off, &above, heap_prompt as u64)
    }

    /// The bottom frame of the placed slice, or 0 when the image was empty.
    /// That frame's offset 0 is the continuation of `do`.
    pub fn resume(&mut self, image: *const u8) -> Result<u64, RtError> {
        let Some(under_off) = self.sp_off else {
            self.install_replace(image)?;
            let bottom = if self.sp_off.is_some() { self.ptr_at(0) as u64 } else { 0 };
            return Ok(bottom);
        };
        let frames = self.image_frames(image);
        let start = under_off + self.frame_nbytes_off(under_off);
        let nbytes: usize = frames.iter().map(|frame| self.frame_nbytes(*frame)).sum();
        self.ensure(start + nbytes)?;
        let under_ptr = self.ptr_at(under_off);
        // The heap image stores 0 so it does not alias the live segment.
        let handlers = read_u64(under_ptr, FRAME_SPILL_HANDLERS);
        let under = under_ptr as u64;
        self.place(start, &frames, &mut Vec::new(), under)?;
        // `start` is an offset, so this is the prompt in the segment `ensure` kept.
        let bottom = if frames.is_empty() { 0 } else { self.ptr_at(start) as u64 };
        if bottom != 0 {
            let bottom_ptr = bottom as *mut u8;
            if self.is_prompt(bottom_ptr) {
                // The outer handler lives on the frame under the slice. The heap image keeps the anchor.
                write_u64(bottom_ptr, FRAME_HANDLER_PREV, handlers);
            }
        }
        if slc_resume_trace.load(Ordering::Relaxed) != 0 {
            let index = slc_resume_log_len.fetch_add(1, Ordering::Relaxed) as usize;
            if index < slc_resume_log.len() {
                slc_resume_log[index].store(self.stack_words(), Ordering::Relaxed);
            }
        }
        Ok(bottom)
    }

    /// Raise the current frame to `words` when the callee is larger. A segment move
    /// rebases interior pointers; heap pointers stay.
    pub fn grow_frame(&mut self, words: u32) -> Result<(), RtError> {
        let sp = self.sp_off.expect("grow needs a frame");
        let bytes = words as usize * 8;
        self.ensure(sp + bytes)?;
        let frame = self.sp();
        if u64::from(words) > read_u64(frame, FRAME_FRAME_WORDS) {
            write_u64(frame, FRAME_FRAME_WORDS, u64::from(words));
        }
        Ok(())
    }

    /// Perform steps 1–2. The copy stops at the prompt whose clauses name `op_id`,
    /// including nearer prompts. The live stack drops that slice. The heap resume
    /// is left in [`slc_split_resume`]; nothing here writes the preserved frame.
    pub fn split_for_perform(&mut self, op_id: u64) -> Result<(), RtError> {
        let mut frame = self.read(self.sp(), FRAME_SPILL_HANDLERS) as *const u8;
        let anchor = anchor_addr();
        let mut handler = None;
        let mut clause = 0u64;
        for _ in 0..1_000_000 {
            if frame.is_null() || frame as u64 == anchor {
                break;
            }
            if self.is_prompt(frame) {
                // The stamp is the barrier under this prompt. A closure born
                // earlier is not aware of an operation that arrived through the row.
                let (_, flags) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
                let barrier = u64::from(flags >> 8);
                let op = label_named(op_id);
                let knows = barrier_table()
                    .get(barrier as usize)
                    .is_some_and(|names| names.iter().any(|name| name == &op));
                if barrier > slc_origin.load(Ordering::Relaxed) && !knows {
                    frame = self.read(frame, FRAME_HANDLER_PREV) as *const u8;
                    continue;
                }
                let clauses = self.read(frame, FRAME_SLOT0);
                if let Some(found) = self.clause_for(clauses, op_id) {
                    handler = Some(frame);
                    clause = found;
                    break;
                }
                frame = self.read(frame, FRAME_HANDLER_PREV) as *const u8;
            } else {
                break;
            }
        }
        let Some(handler) = handler else {
            return Err(RtError::Unhandled(op_id));
        };
        // Record the handler before the slice is dropped. ApplyTo reuses those bytes.
        slc_answered_handler.store(handler as u64, Ordering::Relaxed);
        let resume = self.capture_through(handler);
        let preserved = self.read(handler, FRAME_CONT_PREV) as *mut u8;
        let outer = self.read(handler, FRAME_HANDLER_PREV);
        if preserved.is_null() || preserved as u64 == anchor || !self.in_segment(preserved as u64) {
            return Err(RtError::Unhandled(op_id));
        }
        // The slice keeps the flag bits. The live copies must not leave the
        // barrier raised for the next handler in this frame.
        self.discard_origins_until(preserved);
        self.set_sp(preserved);
        slc_split_resume.store(resume as u64, Ordering::Relaxed);
        slc_split_clause.store(clause, Ordering::Relaxed);
        slc_split_handlers.store(outer, Ordering::Relaxed);
        Ok(())
    }

    fn clause_for(&self, clauses: u64, op_id: u64) -> Option<u64> {
        if clauses == 0 || !self.object_base(clauses).is_some() {
            return None;
        }
        let ptr = clauses as *const u8;
        let count = self.read(ptr, 16);
        for index in 0..count {
            let at = 24 + index as usize * 16;
            if self.read(ptr, at) == op_id {
                return Some(self.read(ptr, at + 8));
            }
        }
        None
    }

    /// Offset 0 of the prompt frame inside a heap `Resume`: the continuation of `do`.
    pub fn resume_cont(&self, image: *const u8) -> u64 {
        let frames = self.image_frames(image);
        let prompt = frames.iter().rev().find(|frame| self.is_prompt(**frame)).copied();
        prompt.map(|frame| self.read(frame, 0)).unwrap_or(0)
    }

    pub fn image_frames(&self, obj: *const u8) -> Vec<*const u8> {
        let base = obj as u64;
        let size = self.object_size(obj) as u64;
        let mut out = Vec::new();
        let mut cur = base + 16;
        let anchor = anchor_addr();
        for _ in 0..1_000_000 {
            if cur < base + 16 || cur + FRAME_HEADER_BYTES as u64 > base + size {
                break;
            }
            let frame = cur as *const u8;
            out.push(frame);
            let prev = read_u64(frame, FRAME_CONT_PREV);
            if prev == anchor || prev == 0 {
                break;
            }
            cur = prev;
        }
        out
    }

    pub fn poll(&mut self) {
        // The Rust fields stay the source of truth. Publish so generated code sees a test's write.
        self.publish_counters();
        if !self.collection_due() {
            return;
        }
        self.collect();
    }

    fn publish_counters(&self) {
        slc_bytes_since_gc.store(self.bytes_since_gc as u64, Ordering::Relaxed);
        slc_watermark.store(self.watermark as u64, Ordering::Relaxed);
    }

    fn collection_due(&self) -> bool {
        self.bytes_since_gc >= self.watermark
    }

    fn alloc_raw(
        &mut self,
        size: usize,
        tag: u16,
        display: u16,
        payload_words: u32,
        map_id: u32,
    ) -> *mut u8 {
        // Id 0 is not a layout, and the object is not a root yet, so do not collect for it.
        if map_id != MAP_UNWRITTEN && self.collection_due() {
            self.collect();
        }
        let size = (size + 15) & !15;
        assert!(size > 0 && size <= u32::MAX as usize, "object size");
        let (chunk_i, ptr) = self.bump_alloc(size);
        unsafe {
            std::ptr::write(
                ptr.cast::<Header>(),
                Header { meta: pack_meta(tag, display, payload_words), mark: MARK_WHITE, map_id },
            );
        }
        // The header is written before the base bit is published.
        self.note(chunk_i, ptr, size);
        self.bytes_since_gc = self.bytes_since_gc.saturating_add(size);
        self.publish_counters();
        self.publish_bump();
        ptr
    }

    /// Copy the cursor written by generated code into this runtime.
    /// The fast path advances `slc_bump` without the lock.
    fn sync_fast_heap(&mut self) {
        if !self.publishing {
            return;
        }
        self.bytes_since_gc = slc_bytes_since_gc.load(Ordering::Relaxed) as usize;
        let bump = slc_bump.load(Ordering::Acquire);
        if bump == 0 || self.bump_index >= self.chunks.len() {
            return;
        }
        let chunk = &mut self.chunks[self.bump_index];
        let base = chunk.ptr as u64;
        let end = base + chunk.len as u64;
        if bump < base || bump > end {
            return;
        }
        let off = (bump - base) as usize;
        self.bump_off = off;
        if off > chunk.used {
            chunk.used = off;
        }
    }

    /// Publish the current chunk's cursor. The bump store is last, so a fast
    /// path that sees it also sees the base, end, and table pointers.
    fn publish_bump(&self) {
        if !self.publishing || self.bump_index >= self.chunks.len() {
            return;
        }
        let chunk = &self.chunks[self.bump_index];
        let base = chunk.ptr as u64;
        slc_chunk_base.store(base, Ordering::Relaxed);
        slc_bump_end.store(base + chunk.len as u64, Ordering::Relaxed);
        slc_heap_bitmap.store(chunk.bitmap.as_ptr() as u64, Ordering::Relaxed);
        slc_heap_slots.store(chunk.slots.as_ptr() as u64, Ordering::Relaxed);
        slc_bump.store(base + self.bump_off as u64, Ordering::Release);
    }

    fn bump_alloc(&mut self, size: usize) -> (usize, *mut u8) {
        if self.bump_index < self.chunks.len()
            && self.bump_off + size <= self.chunks[self.bump_index].len
        {
            let ptr = unsafe { self.chunks[self.bump_index].ptr.add(self.bump_off) };
            self.bump_off += size;
            self.chunks[self.bump_index].used = self.bump_off;
            return (self.bump_index, ptr);
        }
        let len = size.max(CHUNK_BYTES);
        self.add_chunk(len);
        self.bump_index = self.chunks.len() - 1;
        self.bump_off = size;
        self.chunks[self.bump_index].used = size;
        (self.bump_index, self.chunks[self.bump_index].ptr)
    }

    fn add_chunk(&mut self, len: usize) {
        assert!(self.chunks.len() < MAX_CHUNKS, "heap chunk limit");
        let len = (len + 15) & !15;
        let layout = Layout::from_size_align(len, 16).expect("chunk layout");
        let ptr = unsafe { alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "out of memory");
        let slots_n = len / 16;
        let bitmap_words = slots_n.div_ceil(64);
        let bitmap: Box<[AtomicU64]> = (0..bitmap_words).map(|_| AtomicU64::new(0)).collect();
        let slots: Box<[u32]> = vec![0; slots_n].into_boxed_slice();
        self.chunks.push(Chunk { ptr, len, used: 0, layout, bitmap, slots });
        if self.publishing {
            self.publish_chunk(self.chunks.len() - 1);
        }
    }

    fn publish_chunks(&self) {
        for index in 0..self.chunks.len() {
            self.publish_chunk(index);
        }
    }

    fn publish_chunk(&self, index: usize) {
        let chunk = &self.chunks[index];
        let base = chunk.ptr as u64;
        // Widen the span before the chunk becomes visible to `word_tag`.
        PUBLISHED.min.fetch_min(base, Ordering::Release);
        PUBLISHED.max.fetch_max(base + chunk.len as u64, Ordering::Release);
        let bitmap = chunk.bitmap.as_ptr() as *mut AtomicU64;
        PUBLISHED.bitmap[index].store(bitmap, Ordering::Release);
        let mut spans = Vec::with_capacity(index + 1);
        for (i, chunk) in self.chunks.iter().take(index + 1).enumerate() {
            let base = chunk.ptr as u64;
            spans.push(ChunkSpan { base, end: base + chunk.len as u64, index: i as u32 });
        }
        spans.sort_unstable_by_key(|span| span.base);
        // The table is immutable after this store. Older tables stay allocated:
        // a `word_tag` may still be reading one, and the process runtime outlives it.
        let spans: &'static [ChunkSpan] = Box::leak(spans.into_boxed_slice());
        let table = Box::leak(Box::new(SpanTable { len: spans.len(), spans: spans.as_ptr() }));
        PUBLISHED.spans.store(table, Ordering::Release);
    }

    fn capture_image(&mut self, stop: CaptureStop, tag: u16, display: u16) -> *mut u8 {
        let frames = self.live_frames();
        assert!(!frames.is_empty(), "capture of an empty stack");
        let end = match stop {
            CaptureStop::Nearest => frames
                .iter()
                .position(|frame| self.is_prompt(*frame))
                .map_or(frames.len(), |i| i + 1),
            CaptureStop::Outermost => frames
                .iter()
                .rposition(|frame| self.is_prompt(*frame))
                .map_or(frames.len(), |i| i + 1),
            CaptureStop::Through(stop) => frames
                .iter()
                .position(|frame| std::ptr::eq(*frame, stop))
                .map_or(frames.len(), |i| i + 1),
        };
        let copied = &frames[..end];
        let mut cursor = 16usize;
        let mut layout = Vec::with_capacity(copied.len());
        for &frame in copied {
            let nbytes = self.frame_nbytes(frame);
            layout.push((frame, cursor, nbytes));
            cursor += nbytes;
        }
        let payload_words = ((cursor - 16) / 8) as u32;
        let obj = self.alloc_raw(cursor, tag, display, payload_words, MAP_EMPTY);
        for &(frame, off, nbytes) in &layout {
            unsafe { std::ptr::copy_nonoverlapping(frame, obj.add(off), nbytes) }
        }
        let map: Vec<(u64, u64)> =
            layout.iter().map(|&(frame, off, _)| (frame as u64, obj as u64 + off as u64)).collect();
        for (i, &(frame, off, _)) in layout.iter().enumerate() {
            let dst = unsafe { obj.add(off) };
            // Only the outermost copied frame escapes the image; its link is the anchor.
            let cont = if i + 1 == layout.len() {
                anchor_addr()
            } else {
                obj as u64 + layout[i + 1].1 as u64
            };
            write_u64(dst, FRAME_CONT_PREV, cont);
            write_u64(
                dst,
                FRAME_SPILL_HANDLERS,
                self.retarget_out_of_segment(read_u64(frame, FRAME_SPILL_HANDLERS), &map),
            );
            write_u64(
                dst,
                FRAME_HANDLER_PREV,
                self.retarget_out_of_segment(read_u64(frame, FRAME_HANDLER_PREV), &map),
            );
        }
        obj
    }

    fn retarget_out_of_segment(&self, value: u64, map: &[(u64, u64)]) -> u64 {
        if value == 0 {
            return 0;
        }
        if let Some((_, new)) = map.iter().find(|(old, _)| *old == value) {
            return *new;
        }
        // A handler below the cut is not part of the image and must not keep a live address.
        if self.in_segment(value) { 0 } else { value }
    }

    fn install_replace(&mut self, image: *const u8) -> Result<(), RtError> {
        let frames = self.image_frames(image);
        self.discard_origins_until(std::ptr::null());
        if frames.is_empty() {
            self.sp_off = None;
            return Ok(());
        }
        let nbytes: usize = frames.iter().map(|frame| self.frame_nbytes(*frame)).sum();
        self.ensure(nbytes)?;
        self.place(0, &frames, &mut Vec::new(), 0)
    }

    fn install_above(
        &mut self,
        prompt_off: usize,
        above: &[*const u8],
        heap_prompt: u64,
    ) -> Result<(), RtError> {
        // The clause frames above the prompt are not coming back.
        self.discard_origins_until(self.ptr_at(prompt_off));
        if above.is_empty() {
            self.sp_off = Some(prompt_off);
            return Ok(());
        }
        let start = prompt_off + self.frame_nbytes_off(prompt_off);
        let nbytes: usize = above.iter().map(|frame| self.frame_nbytes(*frame)).sum();
        self.ensure(start + nbytes)?;
        let live_prompt = self.ptr_at(prompt_off) as u64;
        let mut map = vec![(heap_prompt, live_prompt)];
        self.place(start, above, &mut map, live_prompt)
    }

    /// `under == 0` keeps an anchor `cont_prev` (the captured chain replaced the stack).
    fn place(
        &mut self,
        start: usize,
        top_first: &[*const u8],
        map: &mut Vec<(u64, u64)>,
        under: u64,
    ) -> Result<(), RtError> {
        if top_first.is_empty() {
            return Ok(());
        }
        let bottom_first: Vec<*const u8> = top_first.iter().copied().rev().collect();
        let mut off = start;
        let mut placed = Vec::with_capacity(bottom_first.len());
        for src in &bottom_first {
            let nbytes = self.frame_nbytes(*src);
            let dst = self.ptr_at(off);
            unsafe { std::ptr::copy_nonoverlapping(*src, dst, nbytes) }
            map.push((*src as u64, dst as u64));
            placed.push((*src as u64, dst));
            off += nbytes;
        }
        let bottom = bottom_first[0] as u64;
        for &(heap, dst) in &placed {
            self.fixup(dst, heap == bottom, map, under);
        }
        let top = placed.last().expect("placed a frame").1;
        self.sp_off = Some(self.offset_of(top));
        // The slice was captured after its saves were popped. Put them back
        // outer-first so a resumed barrier still tunnels.
        for &(_, dst) in &placed {
            self.adopt_origin(dst);
        }
        Ok(())
    }

    fn fixup(&mut self, frame: *mut u8, is_bottom: bool, map: &[(u64, u64)], under: u64) {
        for off in [FRAME_CONT_PREV, FRAME_SPILL_ENV, FRAME_SPILL_HANDLERS, FRAME_HANDLER_PREV] {
            let current = read_u64(frame, off);
            let next =
                if off == FRAME_CONT_PREV && is_bottom && current == anchor_addr() && under != 0 {
                    under
                } else {
                    relocate(current, map)
                };
            if next != current {
                write_u64(frame, off, next);
            }
        }
        let (map_id, _) = unpack_frame_flags(read_u64(frame, FRAME_MAP_FLAGS));
        let record = self.lookup_map(map_id);
        if record.val_is_pointer {
            let current = read_u64(frame, FRAME_SPILL_VAL);
            let next = relocate(current, map);
            if next != current {
                write_u64(frame, FRAME_SPILL_VAL, next);
            }
        }
        let nbytes = read_u64(frame, FRAME_FRAME_WORDS) as usize * 8;
        for &slot in &record.pointer_slots {
            let off = FRAME_SLOT0 + slot as usize * 8;
            if off + 8 > nbytes {
                continue;
            }
            let current = read_u64(frame, off);
            let next = relocate(current, map);
            if next != current {
                write_u64(frame, off, next);
            }
        }
    }

    fn ensure(&mut self, end: usize) -> Result<(), RtError> {
        if end <= self.mem.len() {
            return Ok(());
        }
        let mut new_size = self.mem.len();
        while new_size < end {
            let doubled = new_size.checked_mul(2).ok_or(RtError::StackOverflow)?;
            if doubled <= new_size || doubled > self.segment_cap {
                return Err(RtError::StackOverflow);
            }
            new_size = doubled;
        }
        let mut new_mem = vec![0u8; new_size];
        new_mem[..self.mem.len()].copy_from_slice(&self.mem);
        let old_base = self.mem.as_ptr() as u64;
        let old_len = self.mem.len() as u64;
        let new_base = new_mem.as_mut_ptr() as u64;
        rebase_live(new_mem.as_mut_ptr(), self.sp_off, old_base, old_len, new_base);
        self.mem = new_mem;
        self.publish_limit();
        Ok(())
    }

    /// End of the segment, minus a frame. Generated code compares a new frame to this.
    fn publish_limit(&self) {
        let end = self.mem.as_ptr() as u64 + self.mem.len() as u64;
        slc_segment_limit.store(end.saturating_sub(64 * 1024), Ordering::Relaxed);
    }

    /// Remember the generation this frame is leaving, once. Bits 8..32 hold the
    /// id entered, so a captured copy can adopt it again after the pop.
    fn push_generation(&mut self, entered: u64, barrier: bool) {
        let frame = self.sp();
        let (map, flags) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
        let id = (entered as u32) << 8;
        if flags & FRAME_FLAG_ORIGIN != 0 {
            // One save per frame. A tail call may still move the current origin.
            if barrier {
                let stamped = (flags & 0xff) | FRAME_FLAG_BARRIER | id;
                self.write(frame, FRAME_MAP_FLAGS, pack_frame_flags(map, stamped));
                SLC_BARRIER.store(entered, Ordering::Relaxed);
            } else if flags & FRAME_FLAG_BARRIER == 0 {
                let stamped = (flags & 0xff) | id;
                self.write(frame, FRAME_MAP_FLAGS, pack_frame_flags(map, stamped));
            }
            slc_origin.store(entered, Ordering::Relaxed);
            return;
        }
        origin_saved()
            .push((slc_origin.load(Ordering::Relaxed), SLC_BARRIER.load(Ordering::Relaxed)));
        let mut stamped = (flags & 0xff) | FRAME_FLAG_ORIGIN | id;
        if barrier {
            stamped |= FRAME_FLAG_BARRIER;
            SLC_BARRIER.store(entered, Ordering::Relaxed);
        }
        slc_origin.store(entered, Ordering::Relaxed);
        self.write(frame, FRAME_MAP_FLAGS, pack_frame_flags(map, stamped));
    }

    fn leave_origin_frame(&mut self, frame: *mut u8) {
        let (map, flags) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
        if flags & FRAME_FLAG_ORIGIN == 0 {
            return;
        }
        if let Some((origin, barrier)) = origin_saved().pop() {
            slc_origin.store(origin, Ordering::Relaxed);
            SLC_BARRIER.store(barrier, Ordering::Relaxed);
        }
        let cleared = flags & 0xff & !FRAME_FLAG_ORIGIN & !FRAME_FLAG_BARRIER;
        self.write(frame, FRAME_MAP_FLAGS, pack_frame_flags(map, cleared));
    }

    /// Pop saves for frames above `stop`. `stop` itself stays.
    fn discard_origins_until(&mut self, stop: *const u8) {
        let mut frame = self.sp();
        for _ in 0..1_000_000 {
            if frame.is_null() || std::ptr::eq(frame, stop) || !self.in_segment(frame as u64) {
                break;
            }
            self.leave_origin_frame(frame);
            let prev = self.read(frame, FRAME_CONT_PREV) as *mut u8;
            if prev == frame {
                break;
            }
            frame = prev;
        }
    }

    fn adopt_origin(&mut self, frame: *mut u8) {
        let (_, flags) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
        if flags & FRAME_FLAG_ORIGIN == 0 {
            return;
        }
        let entered = u64::from(flags >> 8);
        origin_saved()
            .push((slc_origin.load(Ordering::Relaxed), SLC_BARRIER.load(Ordering::Relaxed)));
        slc_origin.store(entered, Ordering::Relaxed);
        if flags & FRAME_FLAG_BARRIER != 0 {
            SLC_BARRIER.store(entered, Ordering::Relaxed);
        }
    }

    fn stamp_barrier(&mut self, frame: *mut u8) {
        let (map, flags) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
        let barrier = SLC_BARRIER.load(Ordering::Relaxed) as u32;
        let stamped = (flags & 0xff) | (barrier << 8);
        self.write(frame, FRAME_MAP_FLAGS, pack_frame_flags(map, stamped));
    }

    fn collect(&mut self) {
        for frame in self.live_frames() {
            let (map_id, _) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
            if map_id == MAP_UNWRITTEN {
                panic!("missing stack map");
            }
        }
        self.stats.collections += 1;
        self.each_object(|ptr, _| set_mark(ptr, MARK_WHITE));
        let mut work = Vec::new();
        work.extend(self.immortal.iter().copied());
        work.extend(self.global_table.iter().copied());
        work.extend(self.pointer_pool.iter().copied());
        // Split's resume and clause sit in statics until ApplyTo publishes its map.
        let resume = slc_split_resume.load(Ordering::Relaxed);
        if resume != 0 {
            work.push(resume);
        }
        let clause = slc_split_clause.load(Ordering::Relaxed);
        if clause != 0 {
            work.push(clause);
        }
        if !self.pool_section.is_null() && self.pool_section_len > 0 {
            let words =
                unsafe { std::slice::from_raw_parts(self.pool_section, self.pool_section_len) };
            work.extend_from_slice(words);
        }
        for frame in self.live_frames() {
            work.extend(self.frame_roots(frame));
        }
        while let Some(ptr) = work.pop() {
            self.mark(ptr, &mut work);
        }
        let mut bytes = 0u64;
        let mut count = 0u64;
        for chunk_i in 0..self.chunks.len() {
            let mut slot = 0usize;
            let limit = self.chunks[chunk_i].used / 16;
            while slot < limit {
                let size = self.chunks[chunk_i].slots[slot] as usize;
                if size == 0 {
                    slot += 1;
                    continue;
                }
                let ptr = unsafe { self.chunks[chunk_i].ptr.add(slot * 16) };
                if mark_of(ptr) == MARK_WHITE {
                    self.clear_slot(chunk_i, slot);
                    bytes += size as u64;
                    count += 1;
                } else {
                    set_mark(ptr, MARK_WHITE);
                }
                slot += size / 16;
            }
        }
        self.stats.bytes_swept += bytes;
        self.stats.objects_swept += count;
        self.bytes_since_gc = 0;
        self.publish_counters();
    }

    fn mark(&mut self, ptr: u64, work: &mut Vec<u64>) {
        if ptr == 0 || ptr == anchor_addr() {
            return;
        }
        let Some(base) = self.object_base(ptr) else {
            return;
        };
        if mark_of(base) != MARK_WHITE {
            return;
        }
        set_mark(base, MARK_BLACK);
        work.extend(self.children(base as u64));
    }

    fn children(&self, base: u64) -> Vec<u64> {
        let ptr = base as *const u8;
        let header = self.object_header(ptr);
        let (tag, _, _) = unpack_meta(header.meta);
        if !(TAG_CLOSURE..=TAG_OPERATION).contains(&tag) {
            panic!("unknown heap tag {tag:#x}");
        }
        if tag == TAG_KONT || tag == TAG_RESUME {
            let mut out = Vec::new();
            for frame in self.image_frames(ptr) {
                out.extend(self.frame_roots(frame));
            }
            return out;
        }
        let mut out = self.map_payload_roots(ptr, &header);
        if tag == TAG_CLOSURE || tag == TAG_DELAY {
            let env = self.read(ptr, 24);
            if env != 0 {
                out.push(env);
            }
        } else if tag == TAG_ADAPTED {
            let adapter = self.read(ptr, 16);
            if adapter != 0 {
                out.push(adapter);
            }
        }
        out
    }

    fn map_payload_roots(&self, ptr: *const u8, header: &Header) -> Vec<u64> {
        if header.map_id == MAP_UNWRITTEN {
            panic!("missing stack map");
        }
        let map = self.lookup_map(header.map_id);
        let (_, _, payload_words) = unpack_meta(header.meta);
        let mut out = Vec::new();
        for &slot in &map.pointer_slots {
            if u32::from(slot) >= payload_words {
                continue;
            }
            let word = self.read(ptr, 16 + slot as usize * 8);
            if word != 0 {
                out.push(word);
            }
        }
        out
    }

    fn frame_roots(&self, frame: *const u8) -> Vec<u64> {
        let (map_id, _) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
        let map = self.lookup_map(map_id);
        let mut out = Vec::new();
        for off in [FRAME_SPILL_ENV, FRAME_SPILL_HANDLERS, FRAME_HANDLER_PREV] {
            let word = self.read(frame, off);
            if word != 0 {
                out.push(word);
            }
        }
        if map.val_is_pointer {
            let word = self.read(frame, FRAME_SPILL_VAL);
            if word != 0 {
                out.push(word);
            }
        }
        let nbytes = self.frame_nbytes(frame);
        for &slot in &map.pointer_slots {
            let off = FRAME_SLOT0 + slot as usize * 8;
            if off + 8 <= nbytes {
                let word = self.read(frame, off);
                if word != 0 {
                    out.push(word);
                }
            }
        }
        out
    }

    fn lookup_map(&self, map_id: u32) -> &MapRecord {
        if map_id == MAP_UNWRITTEN {
            panic!("missing stack map");
        }
        if map_id == MAP_EMPTY {
            static EMPTY: MapRecord =
                MapRecord { val_is_pointer: false, pointer_slots: Vec::new() };
            return &EMPTY;
        }
        self.maps.get(&map_id).unwrap_or_else(|| panic!("missing stack map"))
    }

    fn each_object(&self, mut visit: impl FnMut(*mut u8, usize)) {
        for chunk in &self.chunks {
            let mut slot = 0usize;
            let limit = chunk.used / 16;
            while slot < limit {
                let size = chunk.slots[slot] as usize;
                if size == 0 {
                    slot += 1;
                    continue;
                }
                visit(unsafe { chunk.ptr.add(slot * 16) }, size);
                slot += size / 16;
            }
        }
    }

    fn any_tag(&self, want: u16) -> bool {
        let mut found = false;
        self.each_object(|ptr, _| {
            if found {
                return;
            }
            let (tag, _, _) = unpack_meta(self.object_header(ptr).meta);
            found = tag == want;
        });
        found
    }

    /// Record `size` at the object's base slot and publish that bit.
    /// A plain store of the bit word, not a locked `or`: one mutator writes it.
    fn note(&mut self, chunk_i: usize, ptr: *mut u8, size: usize) {
        let chunk = &mut self.chunks[chunk_i];
        let off = (ptr as usize).wrapping_sub(chunk.ptr as usize);
        debug_assert_eq!(off % 16, 0);
        let slot = off / 16;
        chunk.slots[slot] = size as u32;
        let word = slot / 64;
        let bit = slot % 64;
        let bits = chunk.bitmap[word].load(Ordering::Relaxed);
        chunk.bitmap[word].store(bits | (1u64 << bit), Ordering::Release);
    }

    fn clear_slot(&mut self, chunk_i: usize, slot: usize) {
        let chunk = &mut self.chunks[chunk_i];
        let word = slot / 64;
        let bit = slot % 64;
        let bits = chunk.bitmap[word].load(Ordering::Relaxed);
        chunk.bitmap[word].store(bits & !(1u64 << bit), Ordering::Release);
        chunk.slots[slot] = 0;
    }

    fn locate(&self, addr: u64) -> Option<(usize, usize)> {
        let chunk_i = self.chunk_of(addr)?;
        let chunk = &self.chunks[chunk_i];
        let off = (addr - chunk.ptr as u64) as usize;
        if off % 16 != 0 {
            return None;
        }
        Some((chunk_i, off / 16))
    }

    fn chunk_of(&self, addr: u64) -> Option<usize> {
        self.chunks.iter().position(|chunk| {
            let base = chunk.ptr as u64;
            addr >= base && addr < base + chunk.len as u64
        })
    }

    fn slot_size(&self, addr: u64) -> Option<usize> {
        let (chunk_i, slot) = self.locate(addr)?;
        let size = self.chunks[chunk_i].slots[slot];
        if size == 0 { None } else { Some(size as usize) }
    }

    /// The object containing `addr`, or `None` when `addr` is not in the heap.
    /// An exact base is one slot load. An interior address walks back to the
    /// nearest live base in that chunk and checks the object's size.
    fn object_base(&self, addr: u64) -> Option<*mut u8> {
        if addr == 0 {
            return None;
        }
        if self.slot_size(addr).is_some() {
            return Some(addr as *mut u8);
        }
        let chunk_i = self.chunk_of(addr)?;
        let chunk = &self.chunks[chunk_i];
        let base = chunk.ptr as u64;
        if addr >= base + chunk.used as u64 {
            return None;
        }
        let off = (addr - base) as usize;
        let mut slot = off / 16;
        if off % 16 == 0 {
            if slot == 0 {
                return None;
            }
            slot -= 1;
        }
        loop {
            let size = chunk.slots[slot];
            if size != 0 {
                let obj = unsafe { chunk.ptr.add(slot * 16) };
                return (addr < obj as u64 + u64::from(size)).then_some(obj);
            }
            if slot == 0 {
                return None;
            }
            slot -= 1;
        }
    }

    fn live_frames(&self) -> Vec<*const u8> {
        let mut out = Vec::new();
        let mut cur = self.sp();
        let anchor = anchor_addr();
        for _ in 0..1_000_000 {
            if cur.is_null() || cur as u64 == anchor || !self.in_segment(cur as u64) {
                break;
            }
            out.push(cur as *const u8);
            let prev = read_u64(cur, FRAME_CONT_PREV) as *mut u8;
            if prev == cur {
                break;
            }
            cur = prev;
        }
        out
    }

    fn is_prompt(&self, frame: *const u8) -> bool {
        let (_, flags) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
        let id = self.read(frame, FRAME_PROMPT_ID);
        flags & FRAME_FLAG_PROMPT != 0 || id != 0
    }

    fn prompt_from(&self, mut frame: *const u8) -> u64 {
        let anchor = anchor_addr();
        for _ in 0..1_000_000 {
            if frame.is_null() || frame as u64 == anchor || !self.in_segment(frame as u64) {
                return 0;
            }
            if self.is_prompt(frame) {
                return frame as u64;
            }
            frame = self.read(frame, FRAME_CONT_PREV) as *const u8;
        }
        0
    }

    fn frame_nbytes(&self, frame: *const u8) -> usize {
        self.read(frame, FRAME_FRAME_WORDS) as usize * 8
    }

    fn frame_nbytes_off(&self, off: usize) -> usize {
        self.frame_nbytes(self.ptr_at(off))
    }

    fn ptr_at(&self, off: usize) -> *mut u8 {
        unsafe { self.mem.as_ptr().add(off) as *mut u8 }
    }

    fn offset_of(&self, frame: *const u8) -> usize {
        frame as usize - self.mem.as_ptr() as usize
    }

    fn in_segment(&self, addr: u64) -> bool {
        let base = self.mem.as_ptr() as u64;
        addr >= base && addr < base + self.mem.len() as u64
    }
}

fn relocate(value: u64, map: &[(u64, u64)]) -> u64 {
    map.iter().find(|(old, _)| *old == value).map(|(_, new)| *new).unwrap_or(value)
}

fn rebase_live(mem: *mut u8, sp_off: Option<usize>, old_base: u64, old_len: u64, new_base: u64) {
    // Saved interior addresses are not inside a frame field. Compare generations
    // by moving each one with the segment. Heap statics stay put.
    for cell in [&slc_io_frame, &slc_answered_handler, &slc_split_handlers] {
        let value = cell.load(Ordering::Relaxed);
        if value >= old_base && value < old_base + old_len {
            cell.store(new_base + (value - old_base), Ordering::Relaxed);
        }
    }
    let Some(mut off) = sp_off else { return };
    let anchor = anchor_addr();
    for _ in 0..1_000_000 {
        let frame = unsafe { mem.add(off) };
        for field in [FRAME_CONT_PREV, FRAME_SPILL_HANDLERS, FRAME_HANDLER_PREV] {
            let value = read_u64(frame, field);
            if value >= old_base && value < old_base + old_len {
                write_u64(frame, field, new_base + (value - old_base));
            }
        }
        let prev = read_u64(frame, FRAME_CONT_PREV);
        if prev == anchor || prev < new_base || prev >= new_base + old_len {
            break;
        }
        off = (prev - new_base) as usize;
    }
}

fn read_u64(ptr: *const u8, offset: usize) -> u64 {
    unsafe { std::ptr::read_unaligned(ptr.add(offset).cast::<u64>()) }
}

fn write_u64(ptr: *mut u8, offset: usize, value: u64) {
    unsafe { std::ptr::write_unaligned(ptr.add(offset).cast::<u64>(), value) }
}

fn mark_of(ptr: *const u8) -> u32 {
    unsafe { std::ptr::read(ptr.cast::<Header>()).mark }
}

fn set_mark(ptr: *mut u8, mark: u32) {
    unsafe {
        let mut header = std::ptr::read(ptr.cast::<Header>());
        header.mark = mark;
        std::ptr::write(ptr.cast::<Header>(), header);
    }
}

fn anchor_addr() -> u64 {
    std::ptr::from_ref(&slc_rt_prompt_anchor) as u64
}

/// The unit singleton. `Runtime::new` publishes it; `Force` passes it as the delay's argument.
#[unsafe(no_mangle)]
pub static slc_rt_unit: AtomicU64 = AtomicU64::new(0);

/// Non-zero while a linked test records `stack_words` after each `resume`.
#[unsafe(no_mangle)]
pub static slc_resume_trace: AtomicU64 = AtomicU64::new(0);

/// Depth after each traced resume, in call order. Eight entries cover the flat-loop check.
#[unsafe(no_mangle)]
pub static slc_resume_log: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];

#[unsafe(no_mangle)]
pub static slc_resume_log_len: AtomicU64 = AtomicU64::new(0);

/// Prompt whose clauses answered the latest `split`. `write_line` accepts only the startup IO frame.
#[unsafe(no_mangle)]
pub static slc_answered_handler: AtomicU64 = AtomicU64::new(0);

/// Resume object from the latest split. Generated code reads it after `slc_rt_split`.
#[unsafe(no_mangle)]
pub static slc_split_resume: AtomicU64 = AtomicU64::new(0);
#[unsafe(no_mangle)]
pub static slc_split_clause: AtomicU64 = AtomicU64::new(0);
#[unsafe(no_mangle)]
pub static slc_split_handlers: AtomicU64 = AtomicU64::new(0);

/// The IO prompt `slc_rt_start` installed. A copied IO has a different address.
/// Rebased when the segment moves, so it stays the live prompt.
#[unsafe(no_mangle)]
pub static slc_io_frame: AtomicU64 = AtomicU64::new(0);

/// When non-zero, [`Runtime::new`] uses this many bytes instead of 1 MiB.
/// A linked test sets it before `slc_rt_start` so a handler chain can move
/// without filling the default segment.
#[unsafe(no_mangle)]
pub static slc_segment_bytes: AtomicU64 = AtomicU64::new(0);

/// When non-zero, entry collects from the IO frame after a returning `main`.
#[unsafe(no_mangle)]
pub static slc_sweep_on_exit: AtomicU64 = AtomicU64::new(0);

/// Non-zero when a `Kont` is still live after that sweep.
#[unsafe(no_mangle)]
pub static slc_kont_live: AtomicU64 = AtomicU64::new(0);

static LABELS_START: AtomicU64 = AtomicU64::new(0);
static LABELS_STOP: AtomicU64 = AtomicU64::new(0);

/// Fuel left, in safepoints. `slc_rt_start` writes it; generated code decrements it.
#[unsafe(no_mangle)]
pub static slc_fuel: AtomicU64 = AtomicU64::new(0);

/// First byte past the segment, minus 64 KiB, so a frame that starts below it fits.
#[unsafe(no_mangle)]
pub static slc_segment_limit: AtomicU64 = AtomicU64::new(0);

/// Closures built while the program runs are born here. One is the program itself,
/// so a declaration at zero can adopt whoever calls it. Generated code reads it.
#[unsafe(no_mangle)]
pub static slc_origin: AtomicU64 = AtomicU64::new(1);
static SLC_BARRIER: AtomicU64 = AtomicU64::new(0);
static NEXT_BARRIER: AtomicU64 = AtomicU64::new(2);
static ORIGIN_SAVED: std::sync::Mutex<Vec<(u64, u64)>> = std::sync::Mutex::new(Vec::new());
static BARRIER_AWARE: std::sync::Mutex<Vec<Vec<String>>> = std::sync::Mutex::new(Vec::new());

fn origin_saved() -> std::sync::MutexGuard<'static, Vec<(u64, u64)>> {
    ORIGIN_SAVED.lock().unwrap_or_else(|err| err.into_inner())
}

fn barrier_table() -> std::sync::MutexGuard<'static, Vec<Vec<String>>> {
    BARRIER_AWARE.lock().unwrap_or_else(|err| err.into_inner())
}

fn reset_generations() {
    slc_origin.store(1, Ordering::Relaxed);
    SLC_BARRIER.store(0, Ordering::Relaxed);
    NEXT_BARRIER.store(2, Ordering::Relaxed);
    origin_saved().clear();
    barrier_table().clear();
}

/// Bytes allocated since the last collection. Generated safepoints compare this to the watermark.
#[unsafe(no_mangle)]
pub static slc_bytes_since_gc: AtomicU64 = AtomicU64::new(0);

/// Next free byte in the current chunk. Zero means the fast path must call the runtime.
#[unsafe(no_mangle)]
pub static slc_bump: AtomicU64 = AtomicU64::new(0);
/// First byte past the current chunk.
#[unsafe(no_mangle)]
pub static slc_bump_end: AtomicU64 = AtomicU64::new(0);
/// Base of the chunk `slc_bump` allocates from.
#[unsafe(no_mangle)]
pub static slc_chunk_base: AtomicU64 = AtomicU64::new(0);
/// Bitmap of that chunk. One bit per 16-byte slot.
#[unsafe(no_mangle)]
pub static slc_heap_bitmap: AtomicU64 = AtomicU64::new(0);
/// Slot table of that chunk. A live base holds the object size in bytes.
#[unsafe(no_mangle)]
pub static slc_heap_slots: AtomicU64 = AtomicU64::new(0);

/// Collection is due when [`slc_bytes_since_gc`] reaches this. Starts at the sentinel "never".
#[unsafe(no_mangle)]
pub static slc_watermark: AtomicU64 = AtomicU64::new(u64::MAX);

/// `%rsp` after `call slc_program_entry` has pushed its return address.
/// `ret` from that value returns to `slc_rt_start`, not to the slot above the call.
#[unsafe(no_mangle)]
pub static slc_c_sp: AtomicU64 = AtomicU64::new(0);

/// Pool singletons. Entry stores them; `slc_rt_str_cmp` returns one of them.
#[unsafe(no_mangle)]
pub static slc_rt_bool_true: AtomicU64 = AtomicU64::new(0);
#[unsafe(no_mangle)]
pub static slc_rt_bool_false: AtomicU64 = AtomicU64::new(0);

// Weak so a linked object can replace it. Rust tests never call `slc_rt_start`.
std::arch::global_asm!(
    // `.globl` would promote the weak binding to STB_GLOBAL, which the assembler rejects.
    ".weak slc_program_entry",
    ".type slc_program_entry, @function",
    "slc_program_entry:",
    "xor eax, eax",
    "ret",
);

unsafe extern "C" {
    fn slc_program_entry(sp: u64) -> u64;
}

/// Base frame under the IO prompt. No slots.
const ENTRY_BASE_WORDS: u32 = 9;
/// IO prompt: header, clauses, return closure, one traced scratch, one untraced park.
const ENTRY_IO_WORDS: u32 = 13;

/// Load map records, install one prompt frame, and call `slc_program_entry`.
///
/// # Safety
/// The pointer pairs are either null or the live bounds of the named sections.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn slc_rt_start(
    fuel: u64,
    safepoints_start: *const u8,
    safepoints_stop: *const u8,
    maps_start: *const u8,
    maps_stop: *const u8,
    text_start: *const u8,
    text_stop: *const u8,
    scalars_start: *const u8,
    scalars_stop: *const u8,
    ptrs_start: *const u8,
    ptrs_stop: *const u8,
    labels_start: *const u8,
    labels_stop: *const u8,
) -> u64 {
    if fuel == 0 {
        eprintln!("error: evaluation diverged (fuel exhausted)");
        std::process::exit(1);
    }
    reset_generations();
    // The section symbols are arguments so `--gc-sections` keeps the sections.
    std::hint::black_box((
        safepoints_start,
        safepoints_stop,
        text_start,
        text_stop,
        scalars_start,
        scalars_stop,
        labels_start,
        labels_stop,
    ));
    slc_fuel.store(fuel, Ordering::Relaxed);
    slc_resume_log_len.store(0, Ordering::Relaxed);
    LABELS_START.store(labels_start as u64, Ordering::Relaxed);
    LABELS_STOP.store(labels_stop as u64, Ordering::Relaxed);
    load_map_section(maps_start, maps_stop);
    let frame = with_runtime(|rt| {
        if ptrs_start.is_null() || ptrs_stop <= ptrs_start {
            rt.pool_section = std::ptr::null();
            rt.pool_section_len = 0;
        } else {
            let bytes = ptrs_stop as usize - ptrs_start as usize;
            rt.pool_section = ptrs_start.cast();
            rt.pool_section_len = bytes / 8;
        }
        rt.push_frame(ENTRY_BASE_WORDS).expect("base frame");
        let id = rt.fresh_prompt_id();
        rt.push_prompt(id, ENTRY_IO_WORDS).expect("initial frame");
        slc_io_frame.store(rt.sp() as u64, Ordering::Relaxed);
        rt.publish_counters();
        rt.sp() as u64
    });
    let status: u64;
    unsafe {
        // A `clobber_abi` asm is a call site, so `%rsp` is 0 (mod 16) before `call`.
        std::arch::asm!(
            "lea rax, [rsp - 8]",
            "mov qword ptr [rip + {csp}], rax",
            "call {entry}",
            csp = sym slc_c_sp,
            entry = sym slc_program_entry,
            in("rdi") frame,
            lateout("rax") status,
            clobber_abi("sysv64"),
        );
    }
    status
}

/// Fuel hit zero at a safepoint. Same diagnostic as fuel 0 before entry, then
/// back to `slc_rt_start` with status 1. This does not return to the safepoint.
#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_fail() {
    eprintln!("error: evaluation diverged (fuel exhausted)");
    unsafe {
        std::arch::asm!(
            "mov rsp, qword ptr [rip + {csp}]",
            "mov eax, 1",
            "ret",
            csp = sym slc_c_sp,
            options(noreturn),
        );
    }
}

fn load_map_section(start: *const u8, stop: *const u8) {
    if start.is_null() || stop.is_null() || stop <= start {
        return;
    }
    let mut cursor = start as usize;
    let end = stop as usize;
    while end.saturating_sub(cursor) >= std::mem::size_of::<slc_abi::MapRecordHeader>() {
        let header = unsafe { std::ptr::read_unaligned(cursor as *const slc_abi::MapRecordHeader) };
        let slots_at = cursor + std::mem::size_of::<slc_abi::MapRecordHeader>();
        let slots_end = slots_at + header.slot_count as usize * 2;
        if slots_end > end {
            break;
        }
        let mut slots = Vec::with_capacity(header.slot_count as usize);
        for index in 0..header.slot_count as usize {
            let slot = unsafe { std::ptr::read_unaligned((slots_at + index * 2) as *const u16) };
            slots.push(slot);
        }
        with_runtime(|rt| rt.register_map(header.map_id, header.val_is_pointer != 0, &slots));
        cursor = (slots_end + 3) & !3;
    }
}

/// Immortal frame with no slots. Capture parks the outermost prompt here.
#[used]
#[unsafe(no_mangle)]
pub static slc_rt_prompt_anchor: FrameHeader = FrameHeader {
    return_address: 0,
    cont_prev: 0,
    spilled_env: 0,
    spilled_handlers: 0,
    spilled_val: 0,
    map_id_and_flags: 0,
    frame_words: 9,
    prompt_id: 0,
    handler_prev: 0,
};

struct ChunkSpan {
    base: u64,
    end: u64,
    index: u32,
}

/// One snapshot of every published chunk, sorted by base. Replaced wholesale.
struct SpanTable {
    len: usize,
    spans: *const ChunkSpan,
}

/// Chunks of the process runtime. `slc_rt_word_tag` reads these without the lock.
struct PublishedChunks {
    min: AtomicU64,
    max: AtomicU64,
    spans: AtomicPtr<SpanTable>,
    bitmap: [AtomicPtr<AtomicU64>; MAX_CHUNKS],
}

static PUBLISHED: PublishedChunks = PublishedChunks {
    min: AtomicU64::new(u64::MAX),
    max: AtomicU64::new(0),
    spans: AtomicPtr::new(std::ptr::null_mut()),
    bitmap: [const { AtomicPtr::new(std::ptr::null_mut()) }; MAX_CHUNKS],
};

/// Tag of an exact base in `bitmap`, or `0xffff` when that slot is empty.
/// The header write happens before the base bit is released.
fn bitmap_tag(bitmap: *const AtomicU64, word: u64, base: u64) -> u64 {
    if bitmap.is_null() {
        return 0xffff;
    }
    let slot = ((word - base) >> 4) as usize;
    let bits = unsafe { (*bitmap.add(slot >> 6)).load(Ordering::Acquire) };
    if bits & (1u64 << (slot & 63)) == 0 {
        return 0xffff;
    }
    let meta = unsafe { std::ptr::read(word as *const u64) };
    let (tag, _, _) = unpack_meta(meta);
    u64::from(tag)
}

/// Tag of an exact published base, or `0xffff`. The header write happens
/// before the base bit is released, so an acquire load of that bit sees it.
fn published_word_tag(word: u64) -> u64 {
    if word & 15 != 0 {
        return 0xffff;
    }
    let min = PUBLISHED.min.load(Ordering::Acquire);
    let max = PUBLISHED.max.load(Ordering::Acquire);
    if word < min || word >= max {
        return 0xffff;
    }
    let table = PUBLISHED.spans.load(Ordering::Acquire);
    if table.is_null() {
        return 0xffff;
    }
    let table = unsafe { &*table };
    let spans = unsafe { std::slice::from_raw_parts(table.spans, table.len) };
    let mut lo = 0;
    let mut hi = spans.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        let span = &spans[mid];
        if word < span.base {
            hi = mid;
            continue;
        }
        if word >= span.end {
            lo = mid + 1;
            continue;
        }
        let bitmap = PUBLISHED.bitmap[span.index as usize].load(Ordering::Acquire);
        return bitmap_tag(bitmap, word, span.base);
    }
    0xffff
}

fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    static RT: std::sync::LazyLock<std::sync::Mutex<Runtime>> = std::sync::LazyLock::new(|| {
        let mut rt = Runtime::new();
        rt.publishing = true;
        rt.publish_chunks();
        rt.publish_bump();
        std::sync::Mutex::new(rt)
    });
    let mut guard = RT.lock().unwrap_or_else(|err| err.into_inner());
    guard.sync_fast_heap();
    f(&mut guard)
}

/// Carve `size` bytes from the current chunk. Returns 0 when the chunk cannot
/// hold it, and the caller uses [`slc_rt_alloc`]. One mutator; no lock.
#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_try_alloc(size: u64, meta: u64, mark_map: u64) -> u64 {
    if size == 0 || size & 15 != 0 {
        return 0;
    }
    let bump = slc_bump.load(Ordering::Relaxed);
    let end = slc_bump_end.load(Ordering::Relaxed);
    let next = bump.wrapping_add(size);
    if bump == 0 || next < bump || next > end {
        return 0;
    }
    slc_bump.store(next, Ordering::Relaxed);
    let base = slc_chunk_base.load(Ordering::Relaxed);
    let slots = slc_heap_slots.load(Ordering::Relaxed) as *mut u32;
    let bitmap = slc_heap_bitmap.load(Ordering::Relaxed) as *mut u64;
    unsafe {
        std::ptr::write(bump as *mut u64, meta);
        std::ptr::write((bump as *mut u8).add(8).cast::<u64>(), mark_map);
        let slot = ((bump - base) >> 4) as usize;
        *slots.add(slot) = size as u32;
        let word = bitmap.add(slot >> 6);
        *word |= 1u64 << (slot & 63);
    }
    let bytes = slc_bytes_since_gc.load(Ordering::Relaxed).wrapping_add(size);
    slc_bytes_since_gc.store(bytes, Ordering::Relaxed);
    bump
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_alloc(sp: u64, words: u64, tag: u64, map_id: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        rt.alloc(words as u32, tag as u16, map_id as u32) as u64
    })
}

// `C-unwind` so a dev-test panic can cross this frame. The abort staticlib never unwinds.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn slc_rt_poll(sp: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        rt.poll();
        rt.sp() as u64
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_fresh_prompt_id(sp: u64) -> u64 {
    let _ = sp;
    with_runtime(|rt| rt.fresh_prompt_id())
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_current_origin() -> u64 {
    slc_origin.load(Ordering::Relaxed)
}

/// A closure born under another generation adopts that generation for the call.
/// Birth zero, or the generation already current, leaves the frame alone.
#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_enter_birth(sp: u64, birth: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        if birth != 0 && birth != slc_origin.load(Ordering::Relaxed) {
            rt.push_generation(birth, false);
        }
        rt.sp() as u64
    })
}

/// The body of a row-polymorphic call. Handlers it installs are stamped with
/// this id and catch only code born here.
#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_enter_poly(sp: u64, aware: u64) -> u64 {
    // Parse before the runtime lock. `string_bytes` does not take it.
    let names = if aware == 0 {
        Vec::new()
    } else {
        let text = String::from_utf8_lossy(&string_bytes(aware)).into_owned();
        text.split(',').filter(|name| !name.is_empty()).map(|name| name.to_string()).collect()
    };
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        let id = NEXT_BARRIER.fetch_add(1, Ordering::Relaxed);
        {
            let mut table = barrier_table();
            let idx = id as usize;
            if table.len() <= idx {
                table.resize(idx + 1, Vec::new());
            }
            table[idx] = names;
        }
        rt.push_generation(id, true);
        rt.sp() as u64
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_leave_origin(sp: u64) {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        let frame = rt.sp();
        rt.leave_origin_frame(frame);
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_stamp_barrier(sp: u64) {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        let frame = rt.sp();
        rt.stamp_barrier(frame);
    })
}

/// Rebased top in `rax`, second word in `rdx`. `slc_rt_resume` uses this.
#[repr(C)]
pub struct SlcPlace {
    pub sp: u64,
    pub frame: u64,
}

/// Grown `sp`, the frame that did not fit, and the live handler.
/// 24 bytes, so System V returns it through a hidden pointer in `rdi`.
/// `sp`, `frame`, and the handler arrive in `rsi`, `rdx`, and `rcx`.
#[repr(C)]
pub struct SlcBump {
    pub sp: u64,
    pub frame: u64,
    pub handler: u64,
}

const _: () = assert!(std::mem::size_of::<SlcBump>() == 24);
const _: () = assert!(std::mem::offset_of!(SlcBump, sp) == 0);
const _: () = assert!(std::mem::offset_of!(SlcBump, frame) == 8);
const _: () = assert!(std::mem::offset_of!(SlcBump, handler) == 16);

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_bump(sp: u64, frame: u64, handler: u64) -> SlcBump {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        let old_base = rt.mem.as_ptr() as u64;
        let old_len = rt.mem.len() as u64;
        let frame_off = frame.wrapping_sub(old_base) as usize;
        if let Err(err) = rt.ensure(frame_off.saturating_add(64 * 1024)) {
            fail_rt(err);
        }
        rt.publish_limit();
        let new_base = rt.mem.as_ptr() as u64;
        let delta = new_base.wrapping_sub(old_base);
        // A prompt in `rbx` is inside the segment. The spill word is not `rbx`
        // after `install_prompt`, and a frame that has not spilled yet holds
        // whatever the previous frame left there.
        let handler = if handler >= old_base && handler < old_base + old_len {
            new_base + (handler - old_base)
        } else {
            handler
        };
        SlcBump { sp: sp.wrapping_add(delta), frame: frame.wrapping_add(delta), handler }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_stack_words(sp: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        rt.stack_words()
    })
}

/// `|n` is the position of an anonymous sum alternative. Anything else is not.
#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_alt_index(id: u64) -> u64 {
    let name = label_named(id);
    name.strip_prefix('|').and_then(|rest| rest.parse().ok()).unwrap_or(u64::MAX)
}

/// The callee is not a function and not an effect operation. Same line as the
/// chunk machine's lookup, including the driver's `error:` prefix.
#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_unbound(sp: u64, id: u64) -> ! {
    let _ = sp;
    eprintln!("error: unbound variable: {}", label_named(id));
    bail(1);
}

fn label_named(id: u64) -> String {
    let start = LABELS_START.load(Ordering::Relaxed) as *const u8;
    let stop = LABELS_STOP.load(Ordering::Relaxed) as *const u8;
    if start.is_null() || stop.is_null() || stop <= start {
        return format!("#{id}");
    }
    let mut cursor = start;
    let mut index = 0u64;
    while cursor < stop {
        let mut end = cursor;
        while end < stop && unsafe { *end } != 0 {
            end = unsafe { end.add(1) };
        }
        if index == id {
            let bytes =
                unsafe { std::slice::from_raw_parts(cursor, end as usize - cursor as usize) };
            return String::from_utf8_lossy(bytes).into_owned();
        }
        if end >= stop {
            break;
        }
        index += 1;
        cursor = unsafe { end.add(1) };
    }
    format!("#{id}")
}

fn bail(status: i32) -> ! {
    unsafe {
        std::arch::asm!(
            "mov rsp, qword ptr [rip + {csp}]",
            "mov eax, edi",
            "ret",
            csp = sym slc_c_sp,
            in("edi") status,
            options(noreturn),
        );
    }
}

fn fail_rt(err: RtError) -> ! {
    // `Unhandled` already carries the driver's `error:` prefix. The others do not.
    match err {
        RtError::Unhandled(_) => eprintln!("{err}"),
        other => eprintln!("error: {other}"),
    }
    bail(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_capture(sp: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        rt.capture() as u64
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_invoke(sp: u64, image: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        match rt.invoke(image as *const u8) {
            Ok(()) => rt.sp() as u64,
            Err(err) => fail_rt(err),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_resume(sp: u64, image: u64) -> SlcPlace {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        let bottom = match rt.resume(image as *const u8) {
            Ok(bottom) => bottom,
            Err(err) => fail_rt(err),
        };
        SlcPlace { sp: rt.sp() as u64, frame: bottom }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_split(sp: u64, op_id: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        match rt.split_for_perform(op_id) {
            Ok(()) => rt.sp() as u64,
            Err(err) => fail_rt(err),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_resume_cont(sp: u64, image: u64) -> u64 {
    // `rdi` is the live frame, the same as every other `slc_rt_*` entry. The
    // continuation word is `rsi`.
    let _ = sp;
    with_runtime(|rt| rt.resume_cont(image as *const u8))
}

/// Tag of a heap object, or `0xffff` when `word` is not that object's address.
/// A scalar must not be loaded as a header.
///
/// Fresh objects sit in the chunk `publish_bump` released. That check is one
/// range compare; older chunks still use the span table.
#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_word_tag(word: u64) -> u64 {
    if word & 15 != 0 {
        return 0xffff;
    }
    // The bump store is the release. Base, end, and the bitmap were stored first.
    let bump = slc_bump.load(Ordering::Acquire);
    if bump != 0 {
        let base = slc_chunk_base.load(Ordering::Relaxed);
        let end = slc_bump_end.load(Ordering::Relaxed);
        if base != 0 && word >= base && word < end {
            let bitmap = slc_heap_bitmap.load(Ordering::Relaxed) as *const AtomicU64;
            return bitmap_tag(bitmap, word, base);
        }
    }
    published_word_tag(word)
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_grow_frame(sp: u64, words: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        match rt.grow_frame(words as u32) {
            Ok(()) => rt.sp() as u64,
            Err(err) => fail_rt(err),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_exit(sp: u64, status: u64) -> ! {
    let _ = sp;
    let signed = status as i64;
    if signed < i64::from(i32::MIN) || signed > i64::from(i32::MAX) {
        eprintln!("error: type mismatch: EXIT status must fit in i32");
        bail(1);
    }
    bail(signed as i32);
}

fn write_answered(text: u64, newline: bool, op: &str) {
    let handler = slc_answered_handler.load(Ordering::Relaxed);
    let io = slc_io_frame.load(Ordering::Relaxed);
    if handler == 0 || handler != io {
        eprintln!("error: type mismatch: no handler for operation `{op}`");
        bail(1);
    }
    let ptr = text as *const u8;
    let len = read_u64(ptr, 16) as usize;
    let bytes = unsafe { std::slice::from_raw_parts(ptr.add(32), len) };
    let mut out = std::io::stdout().lock();
    use std::io::Write;
    let _ = out.write_all(bytes);
    if newline {
        let _ = out.write_all(b"\n");
    }
    let _ = out.flush();
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_write(sp: u64, text: u64) {
    let _ = sp;
    write_answered(text, false, "write");
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_write_line(sp: u64, text: u64) {
    let _ = sp;
    write_answered(text, true, "write_line");
}

/// The interpreter's `EvalError` line, including the driver's `error:` prefix.
fn type_mismatch(message: &str) -> ! {
    eprintln!("error: type mismatch: {message}");
    bail(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_fail_overflow(sp: u64, op: u64, a: u64, b: u64) -> ! {
    let _ = sp;
    let a = a as i64;
    let b = b as i64;
    let text = match op {
        0 => format!("add({a}, {b})"),
        1 => format!("sub({a}, {b})"),
        2 => format!("mul({a}, {b})"),
        _ => format!("neg({a})"),
    };
    type_mismatch(&format!("arithmetic overflow: {text}"));
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_wrapping_div(sp: u64, a: u64, b: u64) -> u64 {
    let _ = sp;
    let (a, b) = (a as i64, b as i64);
    if b == 0 {
        type_mismatch("division by zero");
    }
    a.wrapping_div(b) as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_wrapping_rem(sp: u64, a: u64, b: u64) -> u64 {
    let _ = sp;
    let (a, b) = (a as i64, b as i64);
    if b == 0 {
        type_mismatch("division by zero");
    }
    a.wrapping_rem(b) as u64
}

/// Rust `a % b` on `f64`. Truncating `roundsd` disagrees on `-0.0` and infinity.
#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_frem(sp: u64, a: u64, b: u64) -> u64 {
    let _ = sp;
    (f64::from_bits(a) % f64::from_bits(b)).to_bits()
}

fn fits(n: i64, lo: i64, hi: i64, width: &str) -> u64 {
    if n < lo || n > hi {
        type_mismatch(&format!("arithmetic overflow: {n} does not fit in {width}"));
    }
    n as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_to_i8(sp: u64, n: u64) -> u64 {
    let _ = sp;
    fits(n as i64, i64::from(i8::MIN), i64::from(i8::MAX), "i8")
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_to_i32(sp: u64, n: u64) -> u64 {
    let _ = sp;
    fits(n as i64, i64::from(i32::MIN), i64::from(i32::MAX), "i32")
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_to_i64(sp: u64, n: u64) -> u64 {
    let _ = sp;
    n
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_to_u8(sp: u64, n: u64) -> u64 {
    let _ = sp;
    fits(n as i64, 0, i64::from(u8::MAX), "u8")
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_to_u32(sp: u64, n: u64) -> u64 {
    let _ = sp;
    fits(n as i64, 0, i64::from(u32::MAX), "u32")
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_to_u64(sp: u64, n: u64) -> u64 {
    let _ = sp;
    fits(n as i64, 0, i64::MAX, "u64")
}

/// Copy the bytes first. Allocation may collect, and a register is not a root.
fn string_bytes(ptr: u64) -> Vec<u8> {
    let ptr = ptr as *const u8;
    let len = read_u64(ptr, STRING_BYTE_LEN) as usize;
    unsafe { std::slice::from_raw_parts(ptr.add(STRING_BYTES), len).to_vec() }
}

fn make_string(rt: &mut Runtime, text: &str) -> *mut u8 {
    let bytes = text.as_bytes();
    let chunks = bytes.len().div_ceil(8);
    let obj = rt.alloc(2 + chunks as u32, TAG_STRING, MAP_EMPTY);
    rt.write(obj, STRING_BYTE_LEN, bytes.len() as u64);
    rt.write(obj, STRING_CHAR_LEN, text.chars().count() as u64);
    for (index, chunk) in bytes.chunks(8).enumerate() {
        let mut word = 0u64;
        for (place, byte) in chunk.iter().enumerate() {
            word |= u64::from(*byte) << (8 * place);
        }
        rt.write(obj, STRING_BYTES + 8 * index, word);
    }
    obj
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_int_to_str(sp: u64, n: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        make_string(rt, &format!("{}", n as i64)) as u64
    })
}

/// `None` means `value` is already the string to return.
fn dynamic_display(value: u64) -> Option<String> {
    with_runtime(|rt| {
        if let Some(obj) = rt.object_base(value)
            && obj as u64 == value
        {
            let (tag, _, _) = unpack_meta(rt.object_header(obj).meta);
            if tag == TAG_STRING {
                return None;
            }
        }
        Some(render_value(rt, value))
    })
}

/// Shape 5. A typed integer, float, char, or file never arrives here.
/// Unit is the word 0 or the unit singleton. A non-zero untagged word is a
/// decimal integer, the same text `Value::display` uses for an integer.
fn render_value(rt: &Runtime, value: u64) -> String {
    if value == 0 || value == rt.unit {
        return "(,)".to_string();
    }
    let Some(obj) = rt.object_base(value) else {
        return scalar_text(value);
    };
    if obj as u64 != value {
        return scalar_text(value);
    }
    let (tag, kind, words) = unpack_meta(rt.object_header(obj).meta);
    match kind {
        DISPLAY_MENU => return "<menu>".to_string(),
        DISPLAY_SELECT => return "<select>".to_string(),
        DISPLAY_CONSUMER => return "<consumer>".to_string(),
        DISPLAY_CONTINUATION => return "<continuation>".to_string(),
        DISPLAY_RESUME => return "<resume>".to_string(),
        DISPLAY_CLOSURE => return "<closure>".to_string(),
        _ => {}
    }
    match tag {
        TAG_STRING => {
            let bytes = string_bytes(value);
            let text = String::from_utf8(bytes)
                .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned());
            format!("{text:?}")
        }
        TAG_DELAY | TAG_ADAPTED => "<delayed>".to_string(),
        TAG_CLOSURE => "<closure>".to_string(),
        TAG_KONT => "<continuation>".to_string(),
        TAG_RESUME => "<resume>".to_string(),
        TAG_TUPLE => {
            let count = rt.read(obj, 16).min(u64::from(words.saturating_sub(1))) as usize;
            let parts: Vec<String> =
                (0..count).map(|index| render_value(rt, rt.read(obj, 24 + 8 * index))).collect();
            format!("({})", parts.join(", "))
        }
        TAG_TAGGED => {
            let label = label_named(rt.read(obj, TAGGED_LABEL));
            let payload = rt.read(obj, TAGGED_PAYLOAD);
            let body = render_value(rt, payload);
            if let Some(rest) = label.strip_prefix('|')
                && rest.parse::<usize>().is_ok()
            {
                format!("::{rest}({body})")
            } else if payload == 0 || payload == rt.unit {
                label
            } else {
                format!("{label}({body})")
            }
        }
        TAG_OPERATION => {
            let name = if words >= 1 { label_named(rt.read(obj, 16)) } else { String::new() };
            if name.is_empty() { "<operation>".to_string() } else { format!("<operation {name}>") }
        }
        TAG_ENV => "<env>".to_string(),
        TAG_CLAUSES => "<clauses>".to_string(),
        _ => scalar_text(value),
    }
}

/// Shape 5 has no class. An untagged word is an integer, not a character.
fn scalar_text(value: u64) -> String {
    format!("{}", value as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_display(sp: u64, value: u64, shape: u64) -> u64 {
    // A string's display is the string. Nothing is allocated.
    if shape == 2 {
        return value;
    }
    // No static class: a delay prints `<delayed>` and is not forced.
    if shape == 5 {
        return match dynamic_display(value) {
            None => value,
            Some(text) => with_runtime(|rt| {
                rt.set_sp(sp as *mut u8);
                make_string(rt, &text) as u64
            }),
        };
    }
    let text = match shape {
        0 => format!("{}", value as i64),
        1 => format!("{}", f64::from_bits(value)),
        3 => char::from_u32(value as u32).unwrap_or('\u{FFFD}').to_string(),
        _ => format!("<file@{value}>"),
    };
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        make_string(rt, &text) as u64
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_str_concat(sp: u64, a: u64, b: u64) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        // Registers are not roots. Hold both strings across the allocation;
        // the collector does not move a live object, so the pointers stay put.
        let keep = rt.immortal.len();
        rt.immortal.push(a);
        rt.immortal.push(b);
        let obj = concat_strings(rt, a, b);
        rt.immortal.truncate(keep);
        obj as u64
    })
}

/// One copy into the result. Character length adds; both sides are already strings.
fn concat_strings(rt: &mut Runtime, a: u64, b: u64) -> *mut u8 {
    let a_ptr = a as *const u8;
    let b_ptr = b as *const u8;
    let a_len = read_u64(a_ptr, STRING_BYTE_LEN) as usize;
    let b_len = read_u64(b_ptr, STRING_BYTE_LEN) as usize;
    let chars = read_u64(a_ptr, STRING_CHAR_LEN) + read_u64(b_ptr, STRING_CHAR_LEN);
    let total = a_len + b_len;
    let chunks = total.div_ceil(8);
    let obj = rt.alloc(2 + chunks as u32, TAG_STRING, MAP_EMPTY);
    rt.write(obj, STRING_BYTE_LEN, total as u64);
    rt.write(obj, STRING_CHAR_LEN, chars);
    unsafe {
        std::ptr::copy_nonoverlapping(a_ptr.add(STRING_BYTES), obj.add(STRING_BYTES), a_len);
        std::ptr::copy_nonoverlapping(
            b_ptr.add(STRING_BYTES),
            obj.add(STRING_BYTES + a_len),
            b_len,
        );
    }
    obj
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_str_cmp(sp: u64, a: u64, b: u64, op: u64) -> u64 {
    let _ = sp;
    let ord = string_bytes(a).cmp(&string_bytes(b));
    let yes = match op {
        0 => ord.is_eq(),
        1 => ord.is_ne(),
        2 => ord.is_lt(),
        3 => ord.is_gt(),
        4 => ord.is_le(),
        _ => ord.is_ge(),
    };
    let slot = if yes { &slc_rt_bool_true } else { &slc_rt_bool_false };
    slot.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_index(sp: u64, s: u64, i: u64) -> u64 {
    let _ = sp;
    let n = i as i64;
    let bytes = string_bytes(s);
    let text = String::from_utf8_lossy(&bytes);
    if let Some(ch) = text.chars().nth(n as usize) {
        return ch as u64;
    }
    type_mismatch(&format!("builtin type mismatch: index {n} out of range"));
}

/// Two integers in `rax`/`rdx`: discriminant, then payload. The generated code cuts.
#[repr(C)]
pub struct Offer {
    pub disc: u64,
    pub payload: u64,
}

fn offer(disc: u64, payload: u64) -> Offer {
    Offer { disc, payload }
}

/// Handles belong to this runtime. The interpreter's thread-local map is a different process.
static OPEN_FILES: std::sync::LazyLock<
    std::sync::Mutex<HashMap<u64, std::io::BufReader<std::fs::File>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

fn files() -> std::sync::MutexGuard<'static, HashMap<u64, std::io::BufReader<std::fs::File>>> {
    OPEN_FILES.lock().unwrap_or_else(|err| err.into_inner())
}

fn heap_word(word: u64) -> bool {
    with_runtime(|rt| rt.object_base(word).is_some_and(|base| base as u64 == word))
}

fn string_text(word: u64) -> Option<String> {
    if !heap_word(word) {
        return None;
    }
    let tag_ok = with_runtime(|rt| {
        let (tag, _, _) = unpack_meta(rt.object_header(word as *const u8).meta);
        tag == TAG_STRING
    });
    if !tag_ok {
        return None;
    }
    let bytes = string_bytes(word);
    Some(
        String::from_utf8(bytes)
            .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned()),
    )
}

fn alloc_text(sp: u64, text: &str) -> u64 {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        make_string(rt, text) as u64
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_substring(sp: u64, s: u64, start: u64, end: u64) -> u64 {
    let Some(text) = string_text(s) else {
        type_mismatch("builtin type mismatch: substring expects (String, i64, i64)");
    };
    let (start, end) = (start as i64, end as i64);
    let chars: Vec<char> = text.chars().collect();
    let a = start.max(0) as usize;
    let b = end.max(0) as usize;
    if a > b || b > chars.len() {
        type_mismatch(&format!(
            "builtin type mismatch: slice range {start}..{end} out of bounds for length {}",
            chars.len()
        ));
    }
    alloc_text(sp, &chars[a..b].iter().collect::<String>())
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_skip_digits(sp: u64, s: u64, pos: u64) -> u64 {
    let _ = sp;
    let Some(text) = string_text(s) else {
        type_mismatch("builtin type mismatch: skip_digits expects (String, i64)");
    };
    let mut i = pos as usize;
    let chars: Vec<char> = text.chars().collect();
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    i as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_skip_ws(sp: u64, s: u64, pos: u64) -> u64 {
    let _ = sp;
    let Some(text) = string_text(s) else {
        type_mismatch("builtin type mismatch: skip_ws expects (String, i64)");
    };
    let mut i = pos as usize;
    let chars: Vec<char> = text.chars().collect();
    while i < chars.len() && matches!(chars[i], ' ' | '\n' | '\r' | '\t') {
        i += 1;
    }
    i as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_str_eq(sp: u64, a: u64, b: u64) -> u64 {
    slc_rt_str_cmp(sp, a, b, 0)
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_char_at(sp: u64, s: u64, index: u64) -> Offer {
    let (Some(text), index) = (string_text(s), index as i64) else {
        type_mismatch("char_at expects (String, i64)");
    };
    match usize::try_from(index).ok().and_then(|i| text.chars().nth(i)) {
        Some(ch) => offer(0, ch as u64),
        None => {
            let msg = format!(
                "index {index} is out of range for a string of length {}",
                text.chars().count()
            );
            offer(1, alloc_text(sp, &msg))
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_find_char(sp: u64, s: u64, from: u64, target: u64) -> Offer {
    let Some(text) = string_text(s) else {
        type_mismatch("find_char expects (String, i64, i64)");
    };
    let (from, target) = (from as i64, target as i64);
    let Some(needle) = u32::try_from(target).ok().and_then(char::from_u32) else {
        type_mismatch(&format!("find_char expects a character code, got {target}"));
    };
    let start = usize::try_from(from).unwrap_or(0);
    match text.chars().enumerate().skip(start).find(|(_, ch)| *ch == needle) {
        Some((index, _)) => offer(0, index as u64),
        None => offer(1, alloc_text(sp, &format!("{needle:?} does not occur from index {from}"))),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_parse_int(sp: u64, word: u64) -> Offer {
    let Some(text) = string_text(word) else {
        type_mismatch(&format!("parse_int expects a String, got {}", word as i64));
    };
    match text.parse::<i64>() {
        Ok(n) => offer(0, n as u64),
        Err(err) => {
            let out_of_range =
                err.to_string().contains("too large") || err.to_string().contains("too small");
            let (disc, reason) = if out_of_range {
                (2, format!("integer out of range: {text:?}"))
            } else {
                (1, format!("not an integer: {text:?}"))
            };
            offer(disc, alloc_text(sp, &reason))
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_read_file(sp: u64, path: u64) -> Offer {
    let Some(path) = string_text(path) else {
        return offer(1, alloc_text(sp, "builtin type mismatch: read_file expects a String path"));
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => offer(0, alloc_text(sp, &text)),
        Err(err) => offer(1, alloc_text(sp, &format!("cannot read {path}: {err}"))),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_open_file(sp: u64, path: u64) -> Offer {
    let Some(path) = string_text(path) else {
        type_mismatch("open_file expects a String");
    };
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(err) => return offer(1, alloc_text(sp, &format!("cannot open {path}: {err}"))),
    };
    let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
    files().insert(id, std::io::BufReader::new(file));
    offer(0, id)
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_read_line(sp: u64, id: u64) -> Offer {
    use std::io::BufRead;
    if heap_word(id) {
        type_mismatch("read_line expects a file handle");
    }
    let read = {
        let mut files = files();
        let Some(reader) = files.get_mut(&id) else {
            type_mismatch(&format!("file handle {id} is not open"));
        };
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(Some(line))
            }
            Err(err) => Err(format!("cannot read from handle {id}: {err}")),
        }
    };
    match read {
        Ok(Some(line)) => offer(0, alloc_text(sp, &line)),
        Ok(None) => offer(1, 0),
        Err(err) => type_mismatch(&err),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_close_file(sp: u64, id: u64) -> u64 {
    let _ = sp;
    if heap_word(id) {
        type_mismatch("builtin type mismatch: close_file expects a file handle");
    }
    if files().remove(&id).is_none() {
        type_mismatch(&format!("file handle {id} is not open"));
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_write_file(sp: u64, path: u64, contents: u64) -> Offer {
    let (Some(path), Some(contents)) = (string_text(path), string_text(contents)) else {
        return offer(
            1,
            alloc_text(sp, "builtin type mismatch: write_file expects (path, content) Strings"),
        );
    };
    match std::fs::write(&path, contents.as_bytes()) {
        Ok(()) => offer(0, 0),
        Err(err) => offer(1, alloc_text(sp, &format!("cannot write {path}: {err}"))),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_file_exists(sp: u64, path: u64) -> u64 {
    let _ = sp;
    let Some(path) = string_text(path) else {
        type_mismatch("builtin type mismatch: file_exists expects a String path");
    };
    let slot =
        if std::path::Path::new(&path).exists() { &slc_rt_bool_true } else { &slc_rt_bool_false };
    slot.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_sweep(sp: u64) {
    with_runtime(|rt| {
        rt.set_sp(sp as *mut u8);
        rt.watermark = 0;
        rt.bytes_since_gc = 1;
        rt.collect();
        let live = rt.any_tag(TAG_KONT);
        slc_kont_live.store(u64::from(live), Ordering::Relaxed);
        rt.watermark = usize::MAX;
        rt.publish_counters();
    });
}

/// # Safety
/// `out` is either null or points at a writable [`GcStats`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn slc_rt_gc_stats(sp: u64, out: *mut GcStats) {
    let _ = sp;
    if out.is_null() {
        return;
    }
    let stats = with_runtime(|rt| rt.gc_stats());
    unsafe { std::ptr::write(out, stats) }
}

#[cfg(test)]
mod tests {
    use super::*;

    std::arch::global_asm!(
        ".globl slc_c_sp_probe",
        ".type slc_c_sp_probe, @function",
        "slc_c_sp_probe:",
        "mov rsp, qword ptr [rip + {csp}]",
        "mov eax, 7",
        "ret",
        csp = sym super::slc_c_sp,
    );

    unsafe extern "C" {
        fn slc_c_sp_probe();
    }

    #[test]
    fn c_sp_points_at_the_return_address() {
        let status: u64;
        unsafe {
            std::arch::asm!(
                "lea rax, [rsp - 8]",
                "mov qword ptr [rip + {csp}], rax",
                "call {probe}",
                csp = sym slc_c_sp,
                probe = sym slc_c_sp_probe,
                lateout("rax") status,
                clobber_abi("sysv64"),
            );
        }
        assert_eq!(status, 7);
    }

    use slc_abi::{
        DISPLAY_CONTINUATION, DISPLAY_RESUME, FRAME_CONT_PREV, FRAME_FLAG_PROMPT,
        FRAME_FRAME_WORDS, FRAME_HANDLER_PREV, FRAME_MAP_FLAGS, FRAME_PROMPT_ID,
        FRAME_RETURN_ADDRESS, FRAME_SLOT0, FRAME_SPILL_ENV, FRAME_SPILL_HANDLERS, FRAME_SPILL_VAL,
        MAP_EMPTY, MARK_WHITE, TAG_CLOSURE, TAG_DELAY, TAG_KONT, TAG_RESUME, TAG_STRING,
        TAG_TAGGED, pack_frame_flags, unpack_frame_flags, unpack_meta,
    };

    fn anchor() -> u64 {
        std::ptr::from_ref(&slc_rt_prompt_anchor) as u64
    }

    #[test]
    fn spill_skips_caller_slots_and_tail_keeps_the_link() {
        let mut rt = Runtime::with_segment_bytes(4096);
        rt.push_frame(12).unwrap();
        let base = rt.sp();
        rt.write(base, FRAME_RETURN_ADDRESS, 0x1111);
        rt.write(base, FRAME_CONT_PREV, 0x2222);
        rt.safepoint_spill(0x10, 0x20, 0x30, 7, FRAME_FLAG_PROMPT);
        assert_eq!(rt.sp(), base);
        assert_eq!(rt.read(base, FRAME_RETURN_ADDRESS), 0x1111);
        assert_eq!(rt.read(base, FRAME_CONT_PREV), 0x2222);
        assert_eq!(rt.read(base, FRAME_SPILL_ENV), 0x10);
        assert_eq!(rt.read(base, FRAME_SPILL_HANDLERS), 0x20);
        assert_eq!(rt.read(base, FRAME_SPILL_VAL), 0x30);
        assert_eq!(rt.read(base, FRAME_MAP_FLAGS), pack_frame_flags(7, FRAME_FLAG_PROMPT));

        rt.tail_slide(0x99, &[0x1, 0x2], 3, 12).unwrap();
        assert_eq!(rt.sp(), base);
        assert_eq!(rt.read(base, FRAME_RETURN_ADDRESS), 0x1111);
        assert_eq!(rt.read(base, FRAME_CONT_PREV), 0x2222);
        assert_eq!(rt.read(base, FRAME_SPILL_ENV), 0x99);
        assert_eq!(rt.read(base, FRAME_SPILL_HANDLERS), 0x20);
        assert_eq!(rt.read(base, FRAME_SPILL_VAL), 0x30);
        assert_eq!(rt.read(base, FRAME_SLOT0), 0x1);
        assert_eq!(rt.read(base, FRAME_SLOT0 + 8), 0x2);
        assert_eq!(unpack_frame_flags(rt.read(base, FRAME_MAP_FLAGS)), (3, FRAME_FLAG_PROMPT));
        assert_eq!(rt.read(base, FRAME_FRAME_WORDS), 12);
    }

    #[test]
    fn split_statics_keep_the_resume_until_they_are_cleared() {
        let _guard = GlobalGuard::arm();
        slc_split_resume.store(0, Ordering::Relaxed);
        slc_split_clause.store(0, Ordering::Relaxed);
        let mut rt = Runtime::with_segment_bytes(256);
        rt.push_frame(9).unwrap();
        let resume = rt.alloc(1, TAG_STRING, MAP_EMPTY);
        let clause = rt.alloc(1, TAG_STRING, MAP_EMPTY);
        rt.set_alloc_watermark(0);
        slc_split_resume.store(resume as u64, Ordering::Relaxed);
        slc_split_clause.store(clause as u64, Ordering::Relaxed);
        rt.poll();
        assert!(rt.is_live(resume));
        assert!(rt.is_live(clause));
        slc_split_resume.store(0, Ordering::Relaxed);
        slc_split_clause.store(0, Ordering::Relaxed);
        rt.poll();
        assert!(!rt.is_live(resume));
        assert!(!rt.is_live(clause));
    }

    #[test]
    fn io_prompt_is_rebased_when_the_segment_moves() {
        let _guard = GlobalGuard::arm();
        slc_io_frame.store(0, Ordering::Relaxed);
        slc_answered_handler.store(0, Ordering::Relaxed);
        let mut rt = Runtime::with_segment_bytes(160);
        rt.push_frame(10).unwrap();
        rt.push_prompt(1, 10).unwrap();
        let io = rt.sp() as u64;
        slc_io_frame.store(io, Ordering::Relaxed);
        let old_base = rt.segment_base() as u64;
        rt.push_frame(10).unwrap();
        let new_base = rt.segment_base() as u64;
        assert_ne!(new_base, old_base);
        let moved = new_base + (io - old_base);
        assert_eq!(slc_io_frame.load(Ordering::Relaxed), moved);
        slc_answered_handler.store(moved, Ordering::Relaxed);
        let text = rt.alloc(3, TAG_STRING, MAP_EMPTY);
        rt.write(text, 16, 3);
        let mut word = 0u64;
        for (index, byte) in b"big".iter().enumerate() {
            word |= u64::from(*byte) << (8 * index);
        }
        rt.write(text, 32, word);
        slc_rt_write_line(0, text as u64);
        slc_io_frame.store(0, Ordering::Relaxed);
        slc_answered_handler.store(0, Ordering::Relaxed);
    }

    #[test]
    fn shape_five_prints_an_untagged_word_as_a_decimal() {
        let rt = Runtime::with_segment_bytes(64);
        assert_eq!(render_value(&rt, 65), "65");
        assert_eq!(render_value(&rt, 0), "(,)");
        assert_eq!(render_value(&rt, rt.unit), "(,)");
    }

    #[test]
    fn bump_rebases_only_a_handler_in_the_old_segment() {
        let _guard = GlobalGuard::arm();
        let (sp, frame, inside, old_base) = with_runtime(|rt| {
            rt.sp_off = None;
            rt.watermark = usize::MAX;
            rt.push_frame(9).unwrap();
            let base = rt.segment_base() as u64;
            let len = rt.mem.len() as u64;
            (rt.sp() as u64, base + len + 64, base + 8, base)
        });
        let moved = slc_rt_bump(sp, frame, inside);
        let new_base = with_runtime(|rt| rt.segment_base() as u64);
        assert_ne!(new_base, old_base);
        assert_eq!(moved.handler, new_base + (inside - old_base));
        assert_eq!(moved.sp, new_base + (sp - old_base));
        assert_eq!(moved.frame, new_base + (frame - old_base));

        let (sp, frame) = with_runtime(|rt| {
            let base = rt.segment_base() as u64;
            let len = rt.mem.len() as u64;
            (rt.sp() as u64, base + len + 64)
        });
        let heap = 0x5151u64;
        let stayed = slc_rt_bump(sp, frame, heap);
        assert_ne!(stayed.sp, sp);
        assert_eq!(stayed.handler, heap);
        let zero = slc_rt_bump(stayed.sp, stayed.frame, 0);
        assert_eq!(zero.handler, 0);
    }

    #[test]
    fn growth_rebases_interior_pointers_not_heap_slots() {
        let mut rt = Runtime::with_segment_bytes(160);
        rt.push_frame(10).unwrap();
        let frame_a = rt.sp();
        rt.push_frame(10).unwrap();
        let frame_b = rt.sp();
        let heap = rt.alloc(1, TAG_STRING, MAP_EMPTY);
        rt.register_map(2, false, &[0]);
        rt.write(frame_b, FRAME_SPILL_ENV, heap as u64);
        rt.write(frame_b, FRAME_SPILL_VAL, heap as u64);
        rt.write(frame_b, FRAME_SLOT0, heap as u64);
        rt.write(frame_b, FRAME_MAP_FLAGS, pack_frame_flags(2, 0));
        rt.write(frame_b, FRAME_SPILL_HANDLERS, frame_a as u64);
        rt.write(frame_b, FRAME_HANDLER_PREV, frame_a as u64);
        let old_base = rt.segment_base() as u64;
        rt.push_frame(10).unwrap();
        let new_base = rt.segment_base() as u64;
        assert_ne!(new_base, old_base);
        let moved_b = rt.read(rt.sp(), FRAME_CONT_PREV);
        assert_eq!(moved_b, new_base + 80);
        assert_eq!(rt.read(moved_b as *const u8, FRAME_CONT_PREV), new_base);
        assert_eq!(rt.read(moved_b as *const u8, FRAME_SPILL_HANDLERS), new_base);
        assert_eq!(rt.read(moved_b as *const u8, FRAME_HANDLER_PREV), new_base);
        assert_eq!(rt.read(moved_b as *const u8, FRAME_SPILL_ENV), heap as u64);
        assert_eq!(rt.read(moved_b as *const u8, FRAME_SPILL_VAL), heap as u64);
        assert_eq!(rt.read(moved_b as *const u8, FRAME_SLOT0), heap as u64);
    }

    #[test]
    fn tail_slide_grows_before_writing_past_the_old_end() {
        let mut rt = Runtime::with_segment_bytes(160);
        rt.push_frame(10).unwrap();
        rt.push_frame(10).unwrap();
        let old_base = rt.segment_base() as u64;
        let sp_off = rt.sp() as u64 - old_base;
        rt.write(rt.sp(), FRAME_RETURN_ADDRESS, 0xA11);
        let prev = rt.read(rt.sp(), FRAME_CONT_PREV);
        let heap = rt.alloc(1, TAG_STRING, MAP_EMPTY) as u64;
        rt.write(rt.sp(), FRAME_SPILL_VAL, heap);
        rt.tail_slide(0xE, &[0x51, 0x52], MAP_EMPTY, 16).unwrap();
        let new_base = rt.segment_base() as u64;
        assert_ne!(new_base, old_base);
        assert_eq!(rt.sp() as u64 - new_base, sp_off);
        assert_eq!(rt.read(rt.sp(), FRAME_RETURN_ADDRESS), 0xA11);
        assert_eq!(rt.read(rt.sp(), FRAME_CONT_PREV), prev - old_base + new_base);
        assert_eq!(rt.read(rt.sp(), FRAME_SPILL_ENV), 0xE);
        assert_eq!(rt.read(rt.sp(), FRAME_SPILL_VAL), heap);
        assert_eq!(rt.read(rt.sp(), FRAME_SLOT0), 0x51);
        assert_eq!(rt.read(rt.sp(), FRAME_SLOT0 + 8), 0x52);
        assert_eq!(rt.read(rt.sp(), FRAME_FRAME_WORDS), 16);
    }

    #[test]
    fn segment_past_cap_is_unchanged() {
        let mut rt = Runtime::with_segment_bytes(160);
        rt.push_frame(10).unwrap();
        rt.push_frame(10).unwrap();
        rt.set_segment_cap(160);
        let base = rt.segment_base();
        let bytes = rt.segment_bytes();
        let err = rt.tail_slide(1, &[2], MAP_EMPTY, 20).unwrap_err();
        assert_eq!(err, RtError::StackOverflow);
        assert_eq!(err.to_string(), "SLC stack overflow");
        assert_eq!(rt.segment_base(), base);
        assert_eq!(rt.segment_bytes(), bytes);
    }

    #[test]
    fn invoke_splices_at_the_inner_prompt() {
        let mut rt = Runtime::new();
        let outer_id = rt.fresh_prompt_id();
        let inner_id = rt.fresh_prompt_id();
        assert_ne!(outer_id, 0);
        rt.push_prompt(outer_id, 10).unwrap();
        let outer = rt.sp();
        rt.push_frame(10).unwrap();
        let middle = rt.sp();
        rt.write(middle, FRAME_SLOT0, 0x51);
        rt.push_prompt(inner_id, 10).unwrap();
        let inner = rt.sp();
        rt.push_frame(10).unwrap();
        let top = rt.sp();
        rt.write(top, FRAME_SLOT0, 0x71);
        rt.write(top, FRAME_SPILL_HANDLERS, inner as u64);
        let segment = rt.segment_bytes();
        let kont = rt.capture();
        assert_eq!(rt.segment_bytes(), segment);
        let (tag, display, _) = unpack_meta(rt.object_header(kont).meta);
        assert_eq!((tag, display), (TAG_KONT, DISPLAY_CONTINUATION));
        let image = rt.image_frames(kont);
        assert_eq!(image.len(), 4);
        let copied_outer = image[3];
        let copied_inner = image[1];
        assert_eq!(rt.read(copied_outer, FRAME_PROMPT_ID), outer_id);
        assert_eq!(rt.read(copied_outer, FRAME_CONT_PREV), anchor());
        assert_eq!(rt.read(copied_inner, FRAME_PROMPT_ID), inner_id);
        assert_ne!(rt.read(copied_inner, FRAME_CONT_PREV), anchor());
        assert_ne!(rt.read(copied_inner, FRAME_CONT_PREV), middle as u64);
        let start = kont as u64;
        let end = start + rt.object_size(kont) as u64;
        let copied_handlers = rt.read(image[0], FRAME_SPILL_HANDLERS);
        assert!(copied_handlers >= start && copied_handlers < end);
        assert_eq!(rt.read(copied_inner, FRAME_HANDLER_PREV), copied_outer as u64);

        rt.write(top, FRAME_SLOT0, 0xD1);
        rt.write(middle, FRAME_SLOT0, 0xD2);
        rt.invoke(kont).unwrap();
        assert_eq!(rt.read(rt.sp(), FRAME_SLOT0), 0x71);
        assert_eq!(rt.read(rt.sp(), FRAME_CONT_PREV), inner as u64);
        assert_eq!(rt.read(rt.sp(), FRAME_SPILL_HANDLERS), inner as u64);
        assert_eq!(rt.read(inner, FRAME_CONT_PREV), middle as u64);
        assert_eq!(rt.read(middle, FRAME_SLOT0), 0xD2);
        assert_eq!(rt.read(middle, FRAME_CONT_PREV), outer as u64);
        assert_eq!(rt.stack_words(), 40);
    }

    #[test]
    fn invoke_under_a_foreign_prompt_does_not_write() {
        let mut rt = Runtime::new();
        let outer_id = rt.fresh_prompt_id();
        let inner_id = rt.fresh_prompt_id();
        rt.push_prompt(outer_id, 10).unwrap();
        rt.push_prompt(inner_id, 10).unwrap();
        rt.push_frame(10).unwrap();
        let kont = rt.capture();
        let foreign = rt.fresh_prompt_id();
        rt.push_prompt(foreign, 10).unwrap();
        let sp = rt.sp();
        let bytes = rt.segment_bytes();
        let err = rt.invoke(kont).unwrap_err();
        assert_eq!(err, RtError::ForeignPrompt);
        assert_eq!(
            err.to_string(),
            "a continuation left the handler it was captured under: it was jumped to under another"
        );
        assert_eq!(rt.sp(), sp);
        assert_eq!(rt.segment_bytes(), bytes);
    }

    #[test]
    fn verdict_keeps_the_live_outer_prompt() {
        let mut rt = Runtime::new();
        rt.push_frame(10).unwrap();
        let below = rt.sp();
        rt.write(below, FRAME_RETURN_ADDRESS, 0xBE10);
        let outer_id = rt.fresh_prompt_id();
        let inner_id = rt.fresh_prompt_id();
        rt.push_prompt(outer_id, 10).unwrap();
        let outer = rt.sp();
        rt.write(outer, FRAME_SLOT0, 0x111);
        rt.push_prompt(inner_id, 10).unwrap();
        rt.push_frame(10).unwrap();
        let kont = rt.capture();
        let image = rt.image_frames(kont);
        assert!(image.iter().any(|frame| rt.read(*frame, FRAME_PROMPT_ID) == outer_id));
        assert!(image.iter().all(|frame| rt.read(*frame, FRAME_RETURN_ADDRESS) != 0xBE10));
        let copied_outer = image
            .iter()
            .copied()
            .find(|frame| rt.read(*frame, FRAME_PROMPT_ID) == outer_id)
            .expect("outer prompt");
        assert_eq!(rt.read(copied_outer, FRAME_CONT_PREV), anchor());
        rt.set_sp(outer);
        rt.write(outer, FRAME_SLOT0, 0x111);
        rt.write(copied_outer as *mut u8, FRAME_SLOT0, 0x222);
        rt.invoke(kont).unwrap();
        assert_eq!(rt.read(outer, FRAME_SLOT0), 0x111);
        assert_eq!(rt.read(outer, FRAME_CONT_PREV), below as u64);
        assert_eq!(rt.read(copied_outer, FRAME_SLOT0), 0x222);
        assert_eq!(rt.read(copied_outer, FRAME_CONT_PREV), anchor());
        let mut frame = rt.sp() as *const u8;
        let mut outer_hits = 0;
        while frame as u64 != anchor() {
            if rt.read(frame, FRAME_PROMPT_ID) == outer_id {
                outer_hits += 1;
                assert_eq!(frame, outer as *const u8);
            }
            frame = rt.read(frame, FRAME_CONT_PREV) as *const u8;
        }
        assert_eq!(outer_hits, 1);
    }

    #[test]
    fn invoke_leaves_the_kont_bytes_unchanged() {
        let mut rt = Runtime::new();
        let outer_id = rt.fresh_prompt_id();
        let inner_id = rt.fresh_prompt_id();
        rt.push_prompt(outer_id, 10).unwrap();
        rt.push_prompt(inner_id, 10).unwrap();
        rt.push_frame(10).unwrap();
        rt.write(rt.sp(), FRAME_SLOT0, 0x71);
        let kont = rt.capture();
        let before = rt.object_bytes(kont).to_vec();
        rt.invoke(kont).unwrap();
        assert_eq!(rt.object_bytes(kont), before.as_slice());
        rt.invoke(kont).unwrap();
        assert_eq!(rt.object_bytes(kont), before.as_slice());
    }

    #[test]
    fn invoke_without_a_prompt_replaces_the_stack() {
        let mut rt = Runtime::new();
        rt.push_frame(10).unwrap();
        rt.write(rt.sp(), FRAME_SLOT0, 0x1);
        rt.push_frame(10).unwrap();
        rt.write(rt.sp(), FRAME_SLOT0, 0x2);
        let kont = rt.capture();
        let before = rt.object_bytes(kont).to_vec();
        rt.write(rt.sp(), FRAME_SLOT0, 0x3);
        rt.invoke(kont).unwrap();
        assert_eq!(rt.object_bytes(kont), before.as_slice());
        assert_eq!(rt.read(rt.sp(), FRAME_SLOT0), 0x2);
        let below = rt.read(rt.sp(), FRAME_CONT_PREV) as *const u8;
        assert_eq!(rt.read(below, FRAME_SLOT0), 0x1);
        assert_eq!(rt.read(below, FRAME_CONT_PREV), anchor());
        assert_eq!(rt.stack_words(), 20);
    }

    #[test]
    fn install_retargets_only_pointer_words() {
        let mut rt = Runtime::new();
        const MAP_VAL: u32 = 2;
        const MAP_SLOT: u32 = 3;
        rt.register_map(MAP_VAL, true, &[]);
        rt.register_map(MAP_SLOT, false, &[0]);
        rt.push_frame(11).unwrap();
        rt.safepoint_spill(0, 0, 0, MAP_VAL, 0);
        rt.push_frame(11).unwrap();
        rt.safepoint_spill(0, 0, 0, MAP_SLOT, 0);
        let kont = rt.capture();
        let image = rt.image_frames(kont);
        let heap_top = image[0];
        let heap_below = image[1];
        let collide = heap_below as u64;
        rt.write(heap_top as *mut u8, FRAME_SPILL_ENV, collide);
        rt.write(heap_top as *mut u8, FRAME_SPILL_VAL, collide);
        rt.write(heap_top as *mut u8, FRAME_SLOT0, collide);
        rt.write(heap_top as *mut u8, FRAME_SLOT0 + 8, collide);
        rt.write(heap_below as *mut u8, FRAME_SPILL_VAL, heap_top as u64);
        let before = rt.object_bytes(kont).to_vec();
        rt.invoke(kont).unwrap();
        assert_eq!(rt.object_bytes(kont), before.as_slice());
        let new_top = rt.sp();
        let new_below = rt.read(new_top, FRAME_CONT_PREV) as *const u8;
        assert_eq!(rt.read(new_top, FRAME_SPILL_ENV), new_below as u64);
        assert_eq!(rt.read(new_top, FRAME_SPILL_VAL), collide);
        assert_eq!(rt.read(new_top, FRAME_SLOT0), new_below as u64);
        assert_eq!(rt.read(new_top, FRAME_SLOT0 + 8), collide);
        assert_eq!(rt.read(new_below, FRAME_SPILL_VAL), new_top as u64);
        assert_eq!(rt.read(new_below, FRAME_CONT_PREV), anchor());
    }

    #[test]
    fn resume_reinstalls_the_bottom_prompt_without_writing_the_heap() {
        let mut rt = Runtime::new();
        rt.push_frame(10).unwrap();
        let base = rt.sp();
        rt.write(base, FRAME_SLOT0, 0xBEEF);
        let id = rt.fresh_prompt_id();
        rt.push_prompt(id, 10).unwrap();
        let prompt = rt.sp();
        rt.write(prompt, FRAME_HANDLER_PREV, base as u64);
        rt.push_frame(10).unwrap();
        let top = rt.sp();
        rt.write(top, FRAME_RETURN_ADDRESS, 0xA11);
        rt.write(top, FRAME_SPILL_HANDLERS, prompt as u64);
        let image = rt.capture_resume();
        let before = rt.object_bytes(image).to_vec();
        let (tag, display, _) = unpack_meta(rt.object_header(image).meta);
        assert_eq!((tag, display), (TAG_RESUME, DISPLAY_RESUME));
        let frames = rt.image_frames(image);
        assert_eq!(frames.len(), 2);
        assert_eq!(rt.read(frames[1], FRAME_PROMPT_ID), id);
        assert_eq!(rt.read(frames[1], FRAME_CONT_PREV), anchor());
        assert_eq!(rt.read(frames[1], FRAME_HANDLER_PREV), 0);
        let start = image as u64;
        let end = start + rt.object_size(image) as u64;
        let handlers = rt.read(frames[0], FRAME_SPILL_HANDLERS);
        assert!(handlers >= start && handlers < end);
        // Perform already stored the outer handler in the caller's HANDLERS word.
        rt.write(top, FRAME_SPILL_HANDLERS, base as u64);
        rt.resume(image).unwrap();
        assert_eq!(rt.object_bytes(image), before.as_slice());
        let new_top = rt.sp();
        assert_ne!(new_top, top);
        assert_eq!(rt.read(new_top, FRAME_RETURN_ADDRESS), 0xA11);
        let new_prompt = rt.read(new_top, FRAME_CONT_PREV) as *const u8;
        assert_eq!(rt.read(new_prompt, FRAME_PROMPT_ID), id);
        assert_eq!(rt.read(new_prompt, FRAME_CONT_PREV), top as u64);
        assert_eq!(rt.read(new_prompt, FRAME_HANDLER_PREV), base as u64);
        assert_eq!(rt.read(frames[1], FRAME_HANDLER_PREV), 0);
        assert_eq!(rt.read(new_top, FRAME_SPILL_HANDLERS), new_prompt as u64);
        assert_eq!(rt.read(top, FRAME_CONT_PREV), prompt as u64);
        assert_eq!(rt.read(prompt, FRAME_CONT_PREV), base as u64);
        assert_eq!(rt.read(base, FRAME_SLOT0), 0xBEEF);
    }

    #[test]
    fn delay_entry_keeps_caller_handlers() {
        let mut rt = Runtime::new();
        rt.push_frame(9).unwrap();
        rt.safepoint_spill(0x1, 0x5151, 0x2, MAP_EMPTY, 0);
        let delay = rt.alloc_delay(0xC0DE, 0xE11E);
        let before = rt.object_bytes(delay).to_vec();
        assert_eq!(before.len(), 32);
        assert_eq!(unpack_meta(rt.object_header(delay).meta).0, TAG_DELAY);
        rt.enter_delay(delay);
        assert_eq!(rt.read(rt.sp(), FRAME_SPILL_HANDLERS), 0x5151);
        assert_eq!(rt.read(rt.sp(), FRAME_SPILL_ENV), 0xE11E);
        assert_eq!(rt.read(rt.sp(), FRAME_SPILL_VAL), rt.unit() as u64);
        assert_eq!(rt.entered_code(), 0xC0DE);
        assert_eq!(rt.object_bytes(delay), before.as_slice());
        assert_eq!(rt.read(delay, 16), 0xC0DE);
        assert_eq!(rt.read(delay, 24), 0xE11E);
    }

    #[test]
    fn cycle_is_swept_and_a_mapped_slot_is_kept() {
        let mut rt = Runtime::new();
        const MAP_FRAME: u32 = 2;
        const MAP_TAGGED: u32 = 3;
        const MAP_CLOSURE: u32 = 4;
        rt.register_map(MAP_FRAME, false, &[0]);
        rt.register_map(MAP_TAGGED, false, &[1]);
        rt.register_map(MAP_CLOSURE, false, &[1]);
        let closure = rt.alloc(3, TAG_CLOSURE, MAP_CLOSURE);
        let tagged = rt.alloc(2, TAG_TAGGED, MAP_TAGGED);
        rt.write(tagged, 24, closure as u64);
        rt.push_frame(10).unwrap();
        rt.safepoint_spill(0, 0, 0, MAP_FRAME, 0);
        rt.write(rt.sp(), FRAME_SLOT0, tagged as u64);
        let kont = rt.capture();
        rt.write(closure, 24, kont as u64);
        let orphan = rt.alloc(1, TAG_STRING, MAP_EMPTY);
        let kept = rt.alloc(1, TAG_STRING, MAP_EMPTY);
        let global_kept = rt.alloc(1, TAG_STRING, MAP_EMPTY);
        let pool_kept = rt.alloc(1, TAG_STRING, MAP_EMPTY);
        rt.write(rt.sp(), FRAME_SLOT0, kept as u64);
        rt.install_globals(&[global_kept as u64]);
        rt.install_pointer_pool(&[pool_kept as u64]);
        let expect = rt.object_size(kont)
            + rt.object_size(tagged)
            + rt.object_size(closure)
            + rt.object_size(orphan);
        rt.set_alloc_watermark(0);
        rt.poll();
        let stats = rt.gc_stats();
        assert_eq!(stats.collections, 1);
        assert_eq!(stats.objects_swept, 4);
        assert_eq!(stats.bytes_swept, expect as u64);
        assert!(!rt.is_live(kont));
        assert!(!rt.is_live(tagged));
        assert!(!rt.is_live(closure));
        assert!(!rt.is_live(orphan));
        assert!(rt.is_live(kept));
        assert!(rt.is_live(global_kept));
        assert!(rt.is_live(pool_kept));
        assert!(rt.is_live(rt.unit()));
    }

    #[test]
    fn object_lookup_uses_the_span_index() {
        let mut rt = Runtime::with_segment_bytes(4096);
        rt.register_map(2, false, &[0]);
        rt.push_frame(10).unwrap();
        rt.safepoint_spill(0, 0, 0, 2, 0);
        let kept = rt.alloc(4, TAG_STRING, MAP_EMPTY);
        let orphan = rt.alloc(2, TAG_STRING, MAP_EMPTY);
        rt.write(rt.sp(), FRAME_SLOT0, kept as u64);
        let kept_addr = kept as u64;
        let past = kept_addr + rt.object_size(kept) as u64;
        assert_eq!(rt.object_base(kept_addr).unwrap() as u64, kept_addr);
        assert_eq!(rt.object_base(kept_addr + 8).unwrap() as u64, kept_addr);
        assert_eq!(rt.object_base(orphan as u64).unwrap(), orphan);
        assert!(rt.object_base(1).is_none());
        if let Some(found) = rt.object_base(past) {
            assert_ne!(found as u64, kept_addr);
        }
        rt.set_alloc_watermark(0);
        rt.poll();
        assert!(rt.is_live(kept));
        assert!(!rt.is_live(orphan));
        assert_eq!(rt.object_base(kept_addr).unwrap() as u64, kept_addr);
        assert_eq!(rt.object_base(kept_addr + 8).unwrap() as u64, kept_addr);
        assert!(rt.object_base(orphan as u64).is_none());
        assert_eq!(rt.object_base(rt.unit() as u64).unwrap() as u64, rt.unit() as u64);
    }

    #[test]
    fn idle_poll_allows_an_unwritten_map() {
        let mut rt = Runtime::with_segment_bytes(256);
        rt.push_frame(9).unwrap();
        rt.write(rt.sp(), FRAME_MAP_FLAGS, pack_frame_flags(0, 0));
        rt.poll();
        assert_eq!(rt.gc_stats().collections, 0);
    }

    #[test]
    #[should_panic(expected = "missing stack map")]
    fn collecting_poll_rejects_an_unwritten_map() {
        let mut rt = Runtime::with_segment_bytes(256);
        rt.push_frame(9).unwrap();
        rt.write(rt.sp(), FRAME_MAP_FLAGS, pack_frame_flags(0, 0));
        rt.set_alloc_watermark(0);
        rt.poll();
    }

    #[test]
    fn alloc_with_an_unwritten_map_does_not_collect() {
        let mut rt = Runtime::with_segment_bytes(256);
        rt.push_frame(9).unwrap();
        rt.write(rt.sp(), FRAME_MAP_FLAGS, pack_frame_flags(0, 0));
        rt.set_alloc_watermark(0);
        let ptr = rt.alloc(1, TAG_STRING, 0);
        assert_eq!(ptr as usize % 16, 0);
        assert_eq!(rt.gc_stats().collections, 0);
        let header = rt.object_header(ptr);
        assert_eq!(header.map_id, 0);
        assert_eq!(header.mark, MARK_WHITE);
        assert_eq!(unpack_meta(header.meta).0, TAG_STRING);
    }

    #[test]
    #[should_panic(expected = "id 0 is not a layout")]
    fn map_zero_is_not_a_layout() {
        let mut rt = Runtime::with_segment_bytes(64);
        rt.register_map(0, false, &[]);
    }

    /// Serializes tests that share the process runtime, and clears `sp` on unwind.
    struct GlobalGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl GlobalGuard {
        fn arm() -> Self {
            static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
            Self { _lock: LOCK.lock().unwrap_or_else(|err| err.into_inner()) }
        }
    }

    impl Drop for GlobalGuard {
        fn drop(&mut self) {
            let _held = &self._lock;
            with_runtime(|rt| {
                rt.sp_off = None;
                rt.watermark = usize::MAX;
            });
        }
    }

    #[test]
    fn word_tag_reads_the_published_bitmap() {
        let _guard = GlobalGuard::arm();
        let (root, kept, orphan) = with_runtime(|rt| {
            rt.sp_off = None;
            rt.watermark = usize::MAX;
            rt.register_map(2, false, &[0]);
            rt.push_frame(10).unwrap();
            let root = rt.sp();
            let kept = rt.alloc(4, TAG_STRING, MAP_EMPTY);
            let orphan = rt.alloc(2, TAG_STRING, MAP_EMPTY);
            rt.write(root, FRAME_SLOT0, kept as u64);
            rt.write(root, FRAME_MAP_FLAGS, pack_frame_flags(2, 0));
            rt.watermark = 0;
            (root as u64, kept as u64, orphan as u64)
        });
        assert_eq!(kept % 16, 0);
        assert_eq!(slc_rt_word_tag(kept), u64::from(TAG_STRING));
        assert_eq!(slc_rt_word_tag(kept + 8), 0xffff);
        assert_eq!(slc_rt_word_tag(orphan), u64::from(TAG_STRING));
        assert_eq!(slc_rt_word_tag(1), 0xffff);
        assert_eq!(slc_rt_poll(root), root);
        assert_eq!(slc_rt_word_tag(kept), u64::from(TAG_STRING));
        assert_eq!(slc_rt_word_tag(orphan), 0xffff);
    }

    /// A live object stays visible after the bump moves to a later chunk.
    #[test]
    fn word_tag_reads_an_older_chunk_after_the_bump_moves() {
        let _guard = GlobalGuard::arm();
        let (old, fresh) = with_runtime(|rt| {
            rt.sp_off = None;
            rt.watermark = usize::MAX;
            rt.push_frame(10).unwrap();
            let old = rt.alloc(4, TAG_STRING, MAP_EMPTY) as u64;
            let then = slc_chunk_base.load(Ordering::Relaxed);
            // Larger than a normal chunk, so this object cannot share `old`'s chunk.
            let _gap = rt.alloc((CHUNK_BYTES as u32) / 8, TAG_TUPLE, MAP_EMPTY);
            let fresh = rt.alloc(2, TAG_ENV, MAP_EMPTY) as u64;
            assert_ne!(slc_chunk_base.load(Ordering::Relaxed), then);
            assert!(fresh >= slc_chunk_base.load(Ordering::Relaxed));
            (old, fresh)
        });
        assert_eq!(slc_rt_word_tag(old), u64::from(TAG_STRING));
        assert_eq!(slc_rt_word_tag(old + 8), 0xffff);
        assert_eq!(slc_rt_word_tag(fresh), u64::from(TAG_ENV));
    }

    /// Generated code advances `slc_bump` without the lock. A later collection
    /// must see that object: exact, marked, and swept when it is not a root.
    #[test]
    fn fast_bump_is_collected_with_the_runtime() {
        let _guard = GlobalGuard::arm();
        with_runtime(|rt| {
            rt.sp_off = None;
            rt.watermark = usize::MAX;
            rt.register_map(2, false, &[0]);
            rt.push_frame(10).unwrap();
            let root = rt.sp();
            if slc_bump_end.load(Ordering::Relaxed).saturating_sub(slc_bump.load(Ordering::Relaxed))
                < 64
            {
                let _pad = rt.alloc(8, TAG_STRING, MAP_EMPTY);
            }
            rt.write(root, FRAME_MAP_FLAGS, pack_frame_flags(2, 0));
            let bump = slc_bump.load(Ordering::Relaxed);
            let base = slc_chunk_base.load(Ordering::Relaxed);
            let slots = slc_heap_slots.load(Ordering::Relaxed) as *mut u32;
            let bitmap = slc_heap_bitmap.load(Ordering::Relaxed) as *mut AtomicU64;
            assert_eq!(bump % 16, 0);
            assert!(bump + 64 <= slc_bump_end.load(Ordering::Relaxed));
            let plant = |at: u64| {
                let obj = at as *mut u8;
                unsafe {
                    std::ptr::write(
                        obj.cast::<Header>(),
                        Header {
                            meta: pack_meta(TAG_STRING, 0, 2),
                            mark: MARK_WHITE,
                            map_id: MAP_EMPTY,
                        },
                    );
                    let slot = ((at - base) >> 4) as usize;
                    *slots.add(slot) = 32;
                    let word = slot / 64;
                    let bit = slot % 64;
                    let prev = (*bitmap.add(word)).load(Ordering::Relaxed);
                    (*bitmap.add(word)).store(prev | (1u64 << bit), Ordering::Release);
                }
                obj
            };
            let kept = plant(bump);
            let orphan = plant(bump + 32);
            slc_bump.store(bump + 64, Ordering::Release);
            slc_bytes_since_gc.fetch_add(64, Ordering::Relaxed);
            rt.write(root, FRAME_SLOT0, kept as u64);
            rt.sync_fast_heap();
            rt.watermark = 0;
            rt.poll();
            assert!(rt.is_live(kept));
            assert_eq!(rt.object_header(kept).mark, MARK_WHITE);
            assert_eq!(rt.object_size(kept), 32);
            assert!(!rt.is_live(orphan));
            assert_eq!(slc_rt_word_tag(kept as u64), u64::from(TAG_STRING));
            assert_eq!(slc_rt_word_tag(orphan as u64), 0xffff);
            rt.watermark = usize::MAX;
            rt.publish_counters();
        });
    }

    #[test]
    fn c_poll_keeps_a_slot_reachable_only_from_the_passed_sp() {
        let _guard = GlobalGuard::arm();
        let (root, kept, orphan) = with_runtime(|rt| {
            rt.sp_off = None;
            rt.watermark = usize::MAX;
            rt.register_map(2, false, &[0]);
            rt.push_frame(10).unwrap();
            let root = rt.sp();
            let kept = rt.alloc(1, TAG_STRING, MAP_EMPTY);
            rt.write(root, FRAME_SLOT0, kept as u64);
            rt.write(root, FRAME_MAP_FLAGS, pack_frame_flags(2, 0));
            rt.push_frame(9).unwrap();
            rt.write(rt.sp(), FRAME_CONT_PREV, anchor());
            rt.write(rt.sp(), FRAME_MAP_FLAGS, pack_frame_flags(0, 0));
            let orphan = rt.alloc(1, TAG_STRING, MAP_EMPTY);
            rt.watermark = 0;
            (root as u64, kept, orphan)
        });
        assert_eq!(slc_rt_poll(root), root);
        with_runtime(|rt| {
            assert!(rt.is_live(kept));
            assert!(!rt.is_live(orphan));
            assert_eq!(rt.sp() as u64, root);
        });
    }

    #[test]
    fn c_alloc_traces_the_passed_frame() {
        let _guard = GlobalGuard::arm();
        let (root, kept, orphan) = with_runtime(|rt| {
            rt.sp_off = None;
            rt.watermark = usize::MAX;
            rt.register_map(2, false, &[0]);
            rt.push_frame(10).unwrap();
            let root = rt.sp();
            let kept = rt.alloc(1, TAG_STRING, MAP_EMPTY);
            rt.write(root, FRAME_SLOT0, kept as u64);
            rt.write(root, FRAME_MAP_FLAGS, pack_frame_flags(2, 0));
            rt.push_frame(9).unwrap();
            rt.safepoint_spill(0, 0, 0, MAP_EMPTY, 0);
            rt.write(rt.sp(), FRAME_CONT_PREV, anchor());
            let orphan = rt.alloc(1, TAG_STRING, MAP_EMPTY);
            rt.watermark = 0;
            (root as u64, kept, orphan)
        });
        let _obj = slc_rt_alloc(root, 1, u64::from(TAG_STRING), u64::from(MAP_EMPTY));
        with_runtime(|rt| {
            assert!(rt.is_live(kept));
            assert!(!rt.is_live(orphan));
        });
    }

    #[test]
    fn c_stack_words_walks_the_passed_frame() {
        let _guard = GlobalGuard::arm();
        let (short, tall) = with_runtime(|rt| {
            rt.sp_off = None;
            rt.watermark = usize::MAX;
            rt.push_frame(10).unwrap();
            let short = rt.sp() as u64;
            rt.push_frame(12).unwrap();
            let tall = rt.sp() as u64;
            (short, tall)
        });
        assert_eq!(slc_rt_stack_words(short), 10);
        assert_eq!(slc_rt_stack_words(tall), 22);
    }

    #[test]
    fn c_poll_allows_an_unwritten_map_when_idle() {
        let _guard = GlobalGuard::arm();
        let (sp, before) = with_runtime(|rt| {
            rt.sp_off = None;
            rt.watermark = usize::MAX;
            rt.push_frame(9).unwrap();
            rt.write(rt.sp(), FRAME_MAP_FLAGS, pack_frame_flags(0, 0));
            (rt.sp() as u64, rt.gc_stats().collections)
        });
        assert_eq!(slc_rt_poll(sp), sp);
        with_runtime(|rt| assert_eq!(rt.gc_stats().collections, before));
    }

    #[test]
    #[should_panic(expected = "missing stack map")]
    fn c_poll_rejects_an_unwritten_map_when_collection_is_due() {
        let _guard = GlobalGuard::arm();
        let sp = with_runtime(|rt| {
            rt.sp_off = None;
            rt.watermark = 0;
            rt.push_frame(9).unwrap();
            rt.write(rt.sp(), FRAME_MAP_FLAGS, pack_frame_flags(0, 0));
            rt.sp() as u64
        });
        let _ = slc_rt_poll(sp);
    }

    #[test]
    fn release_abort_staticlib_exports_runtime_entries() {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest.parent().unwrap().parent().unwrap();
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map_or_else(|| workspace.join("target"), std::path::PathBuf::from);
        let output = std::process::Command::new(env!("CARGO"))
            .current_dir(workspace)
            .args([
                "build",
                "-p",
                "slc-rt",
                "--profile",
                "release-abort",
                "--offline",
                "--target-dir",
            ])
            .arg(&target)
            .output()
            .expect("cargo build");
        assert!(
            output.status.success(),
            "release-abort build failed\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let archive = target.join("release-abort/libslc_rt.a");
        let nm = std::process::Command::new("nm").args(["-g"]).arg(&archive).output().unwrap();
        assert!(nm.status.success(), "{}", String::from_utf8_lossy(&nm.stderr));
        let text = String::from_utf8_lossy(&nm.stdout);
        for name in [
            "slc_rt_alloc",
            "slc_rt_poll",
            "slc_rt_fresh_prompt_id",
            "slc_rt_stack_words",
            "slc_rt_gc_stats",
            "slc_rt_prompt_anchor",
        ] {
            assert!(symbol_in(&text, Some("slc_rt-"), name, true, true), "missing {name}");
        }
        for name in ["__match_dispatch", "__handle"] {
            assert!(!symbol_in(&text, None, name, true, true), "{name} is defined");
        }
        // Prebuilt std is unwind and rustc bundles that object. Nightly `build-std` is
        // the only way to strip its personality. This crate must not reference it.
        assert!(
            !symbol_in(&text, Some("slc_rt-"), "rust_eh_personality", false, false),
            "slc-rt references rust_eh_personality"
        );
        assert!(
            symbol_in(&text, Some("panic_abort-"), "__rust_start_panic", true, false),
            "panic_abort is not linked"
        );
    }

    /// `defined_only` skips undefined (`U`) references. `exact` matches the whole symbol.
    fn symbol_in(
        nm: &str,
        member: Option<&str>,
        name: &str,
        defined_only: bool,
        exact: bool,
    ) -> bool {
        let mut current = "";
        for line in nm.lines() {
            if let Some(header) = line.strip_suffix(':')
                && !header.is_empty()
                && !header.contains(' ')
            {
                current = header;
                continue;
            }
            if member.is_some_and(|prefix| !current.contains(prefix)) {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(first) = parts.next() else { continue };
            let (kind, sym) = if first.chars().all(|c| c.is_ascii_hexdigit()) {
                (parts.next().unwrap_or(""), parts.next())
            } else {
                (first, parts.next())
            };
            if defined_only && kind == "U" {
                continue;
            }
            let hit = sym.is_some_and(|word| {
                if exact { word == name || word == format!("_{name}") } else { word.contains(name) }
            });
            if hit {
                return true;
            }
        }
        false
    }
}
