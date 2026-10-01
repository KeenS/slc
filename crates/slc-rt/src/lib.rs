//! Stack, heap, and mark-sweep for native SLC. Frames are built by hand; nothing here compiles.

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::collections::HashMap;

use slc_abi::{
    DISPLAY_CLOSURE, DISPLAY_CONTINUATION, DISPLAY_RESUME, FRAME_CONT_PREV, FRAME_FLAG_PROMPT,
    FRAME_FRAME_WORDS, FRAME_HANDLER_PREV, FRAME_HEADER_BYTES, FRAME_MAP_FLAGS, FRAME_PROMPT_ID,
    FRAME_SLOT0, FRAME_SPILL_ENV, FRAME_SPILL_HANDLERS, FRAME_SPILL_VAL, FrameHeader, Header,
    MAP_EMPTY, MAP_UNWRITTEN, MARK_BLACK, MARK_WHITE, TAG_ADAPTED, TAG_CLOSURE, TAG_DELAY,
    TAG_KONT, TAG_OPERATION, TAG_RESUME, TAG_TAGGED, pack_frame_flags, pack_meta,
    unpack_frame_flags, unpack_meta,
};

const INITIAL_SEGMENT_BYTES: usize = 1 << 20;
const MAX_SEGMENT_BYTES: usize = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtError {
    StackOverflow,
    ForeignPrompt,
}

impl std::fmt::Display for RtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RtError::StackOverflow => "SLC stack overflow",
            RtError::ForeignPrompt => {
                "a continuation left the handler it was captured under: it was jumped to under another"
            }
        })
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

struct Object {
    ptr: *mut u8,
    size: usize,
    layout: Layout,
}

enum CaptureStop {
    /// Through the outermost prompt.
    Outermost,
    /// Through the nearest prompt. This is the slice a `Resume` reinstates.
    Nearest,
}

pub struct Runtime {
    /// Replaced wholesale on growth so interior pointers stay valid until that copy.
    mem: Vec<u8>,
    segment_cap: usize,
    sp_off: Option<usize>,
    maps: HashMap<u32, MapRecord>,
    objects: Vec<Object>,
    immortal: Vec<u64>,
    global_table: Vec<u64>,
    pointer_pool: Vec<u64>,
    next_prompt: u64,
    watermark: usize,
    bytes_since_gc: usize,
    stats: GcStats,
    unit: u64,
    entered_code: u64,
}

// Owned addresses. The process entry shares one runtime under a mutex.
unsafe impl Send for Runtime {}

impl Drop for Runtime {
    fn drop(&mut self) {
        for obj in self.objects.drain(..) {
            unsafe { dealloc(obj.ptr, obj.layout) }
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
        Self::with_segment_bytes(INITIAL_SEGMENT_BYTES)
    }

    pub fn with_segment_bytes(n: usize) -> Self {
        assert!(n > 0);
        Self::with_cap(n, MAX_SEGMENT_BYTES)
    }

    fn with_cap(n: usize, cap: usize) -> Self {
        let mut rt = Self {
            mem: vec![0u8; n],
            segment_cap: cap,
            sp_off: None,
            maps: HashMap::new(),
            objects: Vec::new(),
            immortal: Vec::new(),
            global_table: Vec::new(),
            pointer_pool: Vec::new(),
            next_prompt: 1,
            watermark: usize::MAX,
            bytes_since_gc: 0,
            stats: GcStats::default(),
            unit: 0,
            entered_code: 0,
        };
        let unit = rt.alloc(2, TAG_TAGGED, MAP_EMPTY);
        rt.unit = unit as u64;
        rt.immortal.push(rt.unit);
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
        self.objects.iter().any(|obj| std::ptr::eq(obj.ptr, ptr))
    }

    pub fn object_size(&self, ptr: *const u8) -> usize {
        self.find_object(ptr).size
    }

    pub fn object_bytes(&self, ptr: *const u8) -> &[u8] {
        let obj = self.find_object(ptr);
        unsafe { std::slice::from_raw_parts(obj.ptr, obj.size) }
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

    pub fn resume(&mut self, image: *const u8) -> Result<(), RtError> {
        let Some(under_off) = self.sp_off else {
            return self.install_replace(image);
        };
        let frames = self.image_frames(image);
        let start = under_off + self.frame_nbytes_off(under_off);
        let nbytes: usize = frames.iter().map(|frame| self.frame_nbytes(*frame)).sum();
        self.ensure(start + nbytes)?;
        let under = self.ptr_at(under_off) as u64;
        self.place(start, &frames, &mut Vec::new(), under)
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
        if !self.collection_due() {
            return;
        }
        self.collect();
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
        let layout = Layout::from_size_align(size, 16).expect("16-byte layout");
        let ptr = unsafe { alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "out of memory");
        unsafe {
            std::ptr::write(
                ptr.cast::<Header>(),
                Header { meta: pack_meta(tag, display, payload_words), mark: MARK_WHITE, map_id },
            );
        }
        self.objects.push(Object { ptr, size, layout });
        self.bytes_since_gc = self.bytes_since_gc.saturating_add(size);
        ptr
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
        Ok(())
    }

    fn fixup(&mut self, frame: *mut u8, is_bottom: bool, map: &[(u64, u64)], under: u64) {
        for off in [
            FRAME_CONT_PREV,
            FRAME_SPILL_ENV,
            FRAME_SPILL_HANDLERS,
            FRAME_SPILL_VAL,
            FRAME_HANDLER_PREV,
        ] {
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
        let nbytes = read_u64(frame, FRAME_FRAME_WORDS) as usize * 8;
        if nbytes < FRAME_SLOT0 {
            return;
        }
        let slots = (nbytes - FRAME_SLOT0) / 8;
        for i in 0..slots {
            let off = FRAME_SLOT0 + i * 8;
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
            new_size = next_segment(new_size, self.segment_cap)?;
        }
        self.grow_to(new_size);
        Ok(())
    }

    fn grow_to(&mut self, new_size: usize) {
        let mut new_mem = vec![0u8; new_size];
        new_mem[..self.mem.len()].copy_from_slice(&self.mem);
        let old_base = self.mem.as_ptr() as u64;
        let old_len = self.mem.len() as u64;
        let new_base = new_mem.as_mut_ptr() as u64;
        rebase_live(new_mem.as_mut_ptr(), self.sp_off, old_base, old_len, new_base);
        self.mem = new_mem;
    }

    fn collect(&mut self) {
        for frame in self.live_frames() {
            let (map_id, _) = unpack_frame_flags(self.read(frame, FRAME_MAP_FLAGS));
            if map_id == MAP_UNWRITTEN {
                panic!("missing stack map");
            }
        }
        self.stats.collections += 1;
        for obj in &self.objects {
            set_mark(obj.ptr, MARK_WHITE);
        }
        let mut work = Vec::new();
        work.extend(self.immortal.iter().copied());
        work.extend(self.global_table.iter().copied());
        work.extend(self.pointer_pool.iter().copied());
        for frame in self.live_frames() {
            work.extend(self.frame_roots(frame));
        }
        while let Some(ptr) = work.pop() {
            self.mark(ptr, &mut work);
        }
        let mut bytes = 0u64;
        let mut count = 0u64;
        let mut i = 0;
        while i < self.objects.len() {
            if mark_of(self.objects[i].ptr) == MARK_WHITE {
                let obj = self.objects.swap_remove(i);
                bytes += obj.size as u64;
                count += 1;
                unsafe { dealloc(obj.ptr, obj.layout) }
            } else {
                set_mark(self.objects[i].ptr, MARK_WHITE);
                i += 1;
            }
        }
        self.stats.bytes_swept += bytes;
        self.stats.objects_swept += count;
        self.bytes_since_gc = 0;
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
        for slot in map.pointer_slots {
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
        for slot in map.pointer_slots {
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

    fn lookup_map(&self, map_id: u32) -> MapRecord {
        if map_id == MAP_UNWRITTEN {
            panic!("missing stack map");
        }
        if map_id == MAP_EMPTY {
            return MapRecord { val_is_pointer: false, pointer_slots: Vec::new() };
        }
        self.maps.get(&map_id).cloned().unwrap_or_else(|| panic!("missing stack map"))
    }

    /// Linear scan over the object list. An interval map is worth it once one heap holds
    /// thousands of objects.
    fn object_base(&self, addr: u64) -> Option<*mut u8> {
        if addr == 0 {
            return None;
        }
        self.objects.iter().find_map(|obj| {
            let base = obj.ptr as u64;
            (addr >= base && addr < base + obj.size as u64).then_some(obj.ptr)
        })
    }

    fn find_object(&self, ptr: *const u8) -> &Object {
        self.objects.iter().find(|obj| std::ptr::eq(obj.ptr, ptr)).expect("live object")
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

fn next_segment(current: usize, cap: usize) -> Result<usize, RtError> {
    let doubled = current.checked_mul(2).ok_or(RtError::StackOverflow)?;
    if doubled <= current || doubled > cap { Err(RtError::StackOverflow) } else { Ok(doubled) }
}

fn relocate(value: u64, map: &[(u64, u64)]) -> u64 {
    map.iter().find(|(old, _)| *old == value).map(|(_, new)| *new).unwrap_or(value)
}

fn rebase_live(mem: *mut u8, sp_off: Option<usize>, old_base: u64, old_len: u64, new_base: u64) {
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

fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    static RT: std::sync::LazyLock<std::sync::Mutex<Runtime>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(Runtime::new()));
    let mut guard = RT.lock().unwrap_or_else(|err| err.into_inner());
    f(&mut guard)
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_alloc(sp: u64, words: u64, tag: u64, map_id: u64) -> u64 {
    let _ = sp;
    with_runtime(|rt| rt.alloc(words as u32, tag as u16, map_id as u32) as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_poll(sp: u64) -> u64 {
    with_runtime(|rt| rt.poll());
    sp
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_fresh_prompt_id(sp: u64) -> u64 {
    let _ = sp;
    with_runtime(|rt| rt.fresh_prompt_id())
}

#[unsafe(no_mangle)]
pub extern "C" fn slc_rt_stack_words(sp: u64) -> u64 {
    let _ = sp;
    with_runtime(|rt| rt.stack_words())
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
        rt.resume(image).unwrap();
        assert_eq!(rt.object_bytes(image), before.as_slice());
        let new_top = rt.sp();
        assert_ne!(new_top, top);
        assert_eq!(rt.read(new_top, FRAME_RETURN_ADDRESS), 0xA11);
        let new_prompt = rt.read(new_top, FRAME_CONT_PREV) as *const u8;
        assert_eq!(rt.read(new_prompt, FRAME_PROMPT_ID), id);
        assert_eq!(rt.read(new_prompt, FRAME_CONT_PREV), top as u64);
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

    #[test]
    fn exports_the_runtime_entry_points_and_not_match_dispatch() {
        let mut stats = GcStats::default();
        assert_ne!(slc_rt_fresh_prompt_id(0), 0);
        unsafe { slc_rt_gc_stats(0, &mut stats) };
        let keep: [*const (); 5] = [
            slc_rt_alloc as *const (),
            slc_rt_poll as *const (),
            slc_rt_stack_words as *const (),
            std::ptr::from_ref(&slc_rt_prompt_anchor).cast(),
            slc_rt_gc_stats as *const (),
        ];
        std::hint::black_box(keep);
        let exe = std::env::current_exe().expect("test binary");
        let output =
            std::process::Command::new("nm").args(["-g", exe.to_str().unwrap()]).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8_lossy(&output.stdout);
        for name in [
            "slc_rt_alloc",
            "slc_rt_poll",
            "slc_rt_fresh_prompt_id",
            "slc_rt_stack_words",
            "slc_rt_gc_stats",
            "slc_rt_prompt_anchor",
        ] {
            assert!(defines(&text, name), "missing {name}\n{text}");
        }
        for name in ["__match_dispatch", "__handle", "slc_rt_start", "slc_rt_unit", "slc_rt_add"] {
            assert!(!defines(&text, name), "{name} is exported\n{text}");
        }
    }

    fn defines(nm: &str, name: &str) -> bool {
        nm.lines().any(|line| {
            line.split_whitespace().any(|word| word == name || word == format!("_{name}"))
        })
    }
}
