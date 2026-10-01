use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use slc_abi::{FRAME_SLOT0, TAGGED_LABEL, TAGGED_PAYLOAD};

use crate::{Cond, Dest, Function, Inst, Module};

fn pipeline(src: &str) -> crate::Compiled {
    let tokens = slc_syntax::lexer::lex(src).unwrap();
    let program = slc_syntax::parser::parse(tokens).unwrap();
    let program = slc_syntax::resolve::resolve_program(&program).unwrap();
    let (program, traits) = slc_syntax::traits::elaborate(&program).unwrap();
    let (dispatch, rows) = slc_check::expr::check_program_with_rows(&program, &traits)
        .unwrap_or_else(|diags| panic!("{diags:?}"));
    assert!(rows.is_empty(), "{rows:?}");
    slc_check::polarity::check_program(&program).unwrap_or_else(|diags| panic!("{diags:?}"));
    slc_check::exhaustive::check_exhaustiveness(&program)
        .unwrap_or_else(|diags| panic!("{diags:?}"));
    let defs = slc_syntax::lower::lower_program_resolving(&program, &dispatch)
        .unwrap_or_else(|err| panic!("{err}"));
    crate::compile(&defs, &dispatch.specializations, &dispatch.payloads)
        .unwrap_or_else(|err| panic!("{err}"))
}

fn flat(func: &Function) -> Vec<&Inst> {
    func.blocks.iter().flat_map(|block| block.insts.iter()).collect()
}

fn runtime_archive() -> PathBuf {
    static CACHE: OnceLock<PathBuf> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let workspace = manifest.parent().unwrap().parent().unwrap().to_path_buf();
            let target = std::env::var_os("CARGO_TARGET_DIR")
                .map_or_else(|| workspace.join("target"), PathBuf::from);
            let output = Command::new(env!("CARGO"))
                .current_dir(&workspace)
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
            target.join("release-abort/libslc_rt.a")
        })
        .clone()
}

fn native_libs() -> Vec<String> {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let output =
                Command::new("rustc").args(["--print", "native-static-libs"]).output().unwrap();
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let line = text
                .lines()
                .find_map(|line| {
                    line.split("native-static-libs:").nth(1).or_else(|| {
                        if line.split_whitespace().any(|word| word.starts_with("-l")) {
                            Some(line)
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or("");
            line.split_whitespace().map(str::to_string).collect()
        })
        .clone()
}

const DRIVER: &str = r#"
#include <stdint.h>
extern uint64_t slc_rt_start(
    uint64_t fuel,
    const void *safepoints_start, const void *safepoints_stop,
    const void *maps_start, const void *maps_stop,
    const void *text_start, const void *text_stop,
    const void *scalars_start, const void *scalars_stop,
    const void *ptrs_start, const void *ptrs_stop,
    const void *labels_start, const void *labels_stop);
extern char __start_slc_safepoints, __stop_slc_safepoints;
extern char __start_slc_maps, __stop_slc_maps;
extern char __start_slc_text, __stop_slc_text;
extern char __start_slc_pool_scalars, __stop_slc_pool_scalars;
extern char __start_slc_pool_ptrs, __stop_slc_pool_ptrs;
extern char __start_slc_labels, __stop_slc_labels;
int main(void) {
    uint64_t status = slc_rt_start(
        ~(uint64_t)0,
        &__start_slc_safepoints, &__stop_slc_safepoints,
        &__start_slc_maps, &__stop_slc_maps,
        &__start_slc_text, &__stop_slc_text,
        &__start_slc_pool_scalars, &__stop_slc_pool_scalars,
        &__start_slc_pool_ptrs, &__stop_slc_pool_ptrs,
        &__start_slc_labels, &__stop_slc_labels);
    return (int)status;
}
"#;

fn link_run(object: &[u8]) -> (i32, PathBuf) {
    let (code, _, dir) = link_run_fuel(object, "~(uint64_t)0");
    (code, dir)
}

fn link_run_fuel(object: &[u8], fuel: &str) -> (i32, String, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "slc-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let object_path = dir.join("p.o");
    std::fs::write(&object_path, object).unwrap();
    std::fs::write(dir.join("main.c"), DRIVER.replace("~(uint64_t)0", fuel)).unwrap();
    let exe = dir.join("p");
    let mut cmd = Command::new("cc");
    cmd.args(["-fPIE", "-pie", "-Wl,--gc-sections", "-o"])
        .arg(&exe)
        .arg(dir.join("main.c"))
        .arg(&object_path)
        .arg(runtime_archive());
    cmd.args(native_libs());
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "link failed\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let ran = Command::new(&exe).output().unwrap();
    (ran.status.code().unwrap_or(127), String::from_utf8_lossy(&ran.stderr).into_owned(), dir)
}

fn nm_text(path: &Path) -> String {
    let output = Command::new("nm").arg(path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn symbol_names(nm: &str) -> Vec<String> {
    nm.lines()
        .filter_map(|line| {
            let cols: Vec<_> = line.split_whitespace().collect();
            match cols.as_slice() {
                [_, _, name] => Some((*name).to_string()),
                [ty, name] if *ty != "U" => Some((*name).to_string()),
                _ => None,
            }
        })
        .collect()
}

#[test]
fn shape_match_returns_the_payload() {
    let compiled = pipeline(
        "enum Color { Red, Blue(i64) }
         func main() -> i64 {
             let color = Color::Blue(7);
             of color { Red => 1, Blue(n) => n }
         }",
    );
    let main = compiled.module.function("main");
    let loads: Vec<_> = flat(main)
        .into_iter()
        .filter(|inst| matches!(inst, Inst::Load { offset, width: 4, .. } if *offset == TAGGED_LABEL as i32))
        .collect();
    assert_eq!(loads.len(), 1);
    assert!(flat(main).iter().any(|inst| matches!(inst, Inst::Ud2)));
    let (status, _) = link_run(&compiled.object);
    assert_eq!(status, 7);
}

#[test]
fn literal_default_is_not_tested_first() {
    let hit = pipeline("func main() -> i64 { of 0 { 0 => 7, _ => 9 } }");
    let main = hit.module.function("main");
    let cmp = flat(main).into_iter().find_map(|inst| match inst {
        Inst::CmpJcc { right, cond, target, .. } => Some((*right, *cond, *target)),
        _ => None,
    });
    let (value, cond, target) = cmp.expect("compare");
    assert_eq!(value, 0);
    assert_eq!(cond, Cond::Ne);
    assert!(matches!(main.blocks[target].insts.first(), Some(Inst::Imm { value: 9, .. })));
    assert_eq!(link_run(&hit.object).0, 7);
    let miss = pipeline("func main() -> i64 { of 1 { 0 => 7, _ => 9 } }");
    assert_eq!(link_run(&miss.object).0, 9);
}

#[test]
fn shared_prefix_switches_once_per_constructor() {
    let compiled = pipeline(
        "enum P { A, B }
         enum Q { X, Y }
         func main() -> i64 {
             of (P::A, Q::Y) {
                 (P::A, Q::X) => 1,
                 (P::A, Q::Y) => 2,
                 (P::B, Q::X) => 3,
                 (P::B, Q::Y) => 4,
                 _ => 0,
             }
         }",
    );
    let main = compiled.module.function("main");
    let insts = flat(main);
    let loads = |offset: i32| {
        insts
            .iter()
            .filter(
                |inst| matches!(inst, Inst::Load { offset: off, width: 8, .. } if *off == offset),
            )
            .count()
    };
    assert_eq!(loads(TAGGED_PAYLOAD as i32), 1);
    assert_eq!(loads(32), 1);
    let cmps: Vec<i64> = insts
        .iter()
        .filter_map(|inst| match inst {
            Inst::CmpJcc { right, .. } => Some(*right),
            _ => None,
        })
        .collect();
    assert_eq!(cmps.iter().filter(|value| **value == 0).count(), 1, "{cmps:?}");
    assert_eq!(cmps.iter().filter(|value| **value == 1).count(), 1, "{cmps:?}");
    assert_eq!(link_run(&compiled.object).0, 2);
}

#[test]
fn or_pattern_shares_one_body_and_range_is_one_test() {
    let or_hit = pipeline("func main() -> i64 { of 1 { 0 | 1 => 7, _ => 9 } }");
    let main = or_hit.module.function("main");
    let jumps: Vec<_> = flat(main)
        .into_iter()
        .filter_map(|inst| match inst {
            Inst::CmpJcc { right, cond: Cond::E, target, .. } => Some((*right, *target)),
            _ => None,
        })
        .collect();
    assert!(
        jumps.contains(&(0, jumps[0].1))
            && jumps.iter().any(|(value, target)| *value == 1 && *target == jumps[0].1)
    );
    let body = &main.blocks[jumps[0].1];
    assert!(body.insts.iter().any(|inst| matches!(inst, Inst::Imm { value: 7, .. })));
    assert!(
        !body.insts.iter().any(|inst| matches!(inst, Inst::CmpJcc { .. } | Inst::InRange { .. }))
    );
    assert_eq!(link_run(&or_hit.object).0, 7);
    let or_miss = pipeline("func main() -> i64 { of 3 { 0 | 1 => 7, _ => 9 } }");
    assert_eq!(link_run(&or_miss.object).0, 9);

    let range_hit = pipeline("func main() -> i64 { of 3 { 2..=4 => 7, _ => 9 } }");
    let ranges: Vec<_> = flat(range_hit.module.function("main"))
        .into_iter()
        .filter_map(|inst| match inst {
            Inst::InRange { lo, hi, .. } => Some((*lo, *hi)),
            _ => None,
        })
        .collect();
    assert_eq!(ranges, vec![(2, 4)]);
    assert_eq!(link_run(&range_hit.object).0, 7);
    let range_miss = pipeline("func main() -> i64 { of 5 { 2..=4 => 7, _ => 9 } }");
    assert_eq!(link_run(&range_miss.object).0, 9);
}

#[test]
fn by_name_payload_is_not_forced() {
    let compiled = pipeline(
        "enum Wrap { Hold((i64 -> i64)) }
         func main() -> i64 {
             let w = Wrap::Hold({ fn(x: i64) -> i64 { x } });
             of w { Hold(x) => 7 }
         }",
    );
    let main = compiled.module.function("main");
    assert!(flat(main).iter().any(|inst| matches!(inst, Inst::Ud2)));
    assert!(
        flat(main).iter().any(|inst| {
            matches!(inst, Inst::CallAlloc { tag, .. } if *tag == slc_abi::TAG_DELAY)
        })
    );
    assert_eq!(link_run(&compiled.object).0, 7);
}

#[test]
fn tail_call_is_not_call_slc() {
    let compiled = pipeline(
        "func id(x: i64) -> i64 { x }
         func calls(x: i64) -> i64 { let y = <x | id; y }
         func main() -> i64 { <42 | id }",
    );
    let main = flat(compiled.module.function("main"));
    assert!(main.iter().any(|inst| matches!(inst, Inst::Tail { symbol, .. } if symbol == "id")));
    assert!(!main.iter().any(|inst| matches!(inst, Inst::CallSlc { .. })));
    let calls = flat(compiled.module.function("calls"));
    assert!(
        calls.iter().any(|inst| matches!(inst, Inst::CallSlc { symbol, .. } if symbol == "id"))
    );
    assert!(!calls.iter().any(|inst| matches!(inst, Inst::Tail { .. })));
    assert_eq!(link_run(&compiled.object).0, 42);
}

#[test]
fn generic_identity_is_two_copies() {
    let compiled = pipeline(
        "enum Flag { Yes, No }
         func id<+T>(x: T) -> T { x }
         func main() -> i64 {
             let n = <41 | id;
             let f = <Flag::Yes | id;
             of f { Yes => n, No => 0 }
         }",
    );
    let i64_copy = compiled.module.function("id$i64");
    let flag_copy = compiled.module.function("id$Flag");
    assert!(!compiled.module.functions.iter().any(|func| func.symbol == "id"));
    assert!(!i64_copy.val_is_pointer);
    assert!(!i64_copy.pointer_slots.contains(&0));
    assert!(flag_copy.val_is_pointer);
    assert!(flag_copy.pointer_slots.contains(&0));
    assert_ne!(i64_copy.map_id, flag_copy.map_id);
    assert_ne!(i64_copy.map_id, 0);
    let main = flat(compiled.module.function("main"));
    assert!(
        main.iter().any(|inst| matches!(inst, Inst::CallSlc { symbol, .. } if symbol == "id$i64"))
    );
    assert!(
        main.iter().any(|inst| matches!(inst, Inst::CallSlc { symbol, .. } if symbol == "id$Flag"))
    );
    let (status, dir) = link_run(&compiled.object);
    assert_eq!(status, 41);
    let names = symbol_names(&nm_text(&dir.join("p.o")));
    assert!(names.iter().any(|name| name == "id$i64"), "{names:?}");
    assert!(names.iter().any(|name| name == "id$Flag"), "{names:?}");
    assert!(!names.iter().any(|name| name == "id"), "{names:?}");
    assert!(!nm_text(&dir.join("p.o")).contains("__match_dispatch"));
    let pie = Command::new("readelf").args(["-S"]).arg(dir.join("p")).output().unwrap();
    let sections = String::from_utf8_lossy(&pie.stdout);
    for name in [
        "slc_text",
        "slc_safepoints",
        "slc_maps",
        "slc_pool_scalars",
        "slc_pool_ptrs",
        "slc_labels",
    ] {
        assert!(sections.contains(name), "{sections}");
    }
}

#[test]
fn spill_slots_use_the_frame_not_a_protocol_register() {
    let module = Module {
        functions: vec![Function {
            symbol: "spill".into(),
            map_id: 2,
            frame_words: 20,
            val_is_pointer: false,
            pointer_slots: vec![],
            spill_base: 7,
            blocks: vec![crate::Block {
                insts: vec![Inst::Mov { dst: Dest::V(10), src: Dest::V(9) }, Inst::Ret],
            }],
            entry: false,
        }],
        labels: vec![],
        pool_len: 0,
        maps: vec![crate::MapRecord {
            map_id: 2,
            frame_words: 20,
            val_is_pointer: false,
            slots: vec![],
        }],
    };
    let object = crate::encode(&module);
    // V(9) is [r12+128] and V(10) is [r12+136]. A mem-to-mem move goes through
    // rax (caller-saved), so the prefix is REX.WB not REX.WR. 104 would be a disp8.
    let needle = [
        0x49, 0x8B, 0x84, 0x24, 0x80, 0x00, 0x00, 0x00, 0x49, 0x89, 0x84, 0x24, 0x88, 0x00, 0x00,
        0x00,
    ];
    assert!(object.windows(needle.len()).any(|window| window == needle));
}

#[test]
fn later_row_that_accepts_the_range_still_runs() {
    let compiled = pipeline(
        "enum E { A(i64) }
         func main() -> i64 {
             of (E::A(1), E::A(2)) {
                 (E::A(0..=1), E::A(0)) => 1,
                 (E::A(0..=1), _) => 2,
                 _ => 3,
             }
         }",
    );
    assert_eq!(link_run(&compiled.object).0, 2);
}

#[test]
fn constructor_or_is_one_row() {
    let compiled = pipeline(
        "enum C { Red, Blue, Green }
         func main() -> i64 {
             of C::Blue { Red | Blue => 7, Green => 8 }
         }",
    );
    let main = compiled.module.function("main");
    let jumps: Vec<_> = flat(main)
        .into_iter()
        .filter_map(|inst| match inst {
            Inst::CmpJcc { cond: Cond::E, target, .. } => Some(*target),
            _ => None,
        })
        .collect();
    assert!(jumps.len() >= 2);
    assert_eq!(jumps[0], jumps[1]);
    assert!(
        main.blocks[jumps[0]].insts.iter().any(|inst| matches!(inst, Inst::Imm { value: 7, .. }))
    );
    assert_eq!(link_run(&compiled.object).0, 7);
}

#[test]
fn pointer_bits_follow_the_stored_word() {
    let compiled = pipeline(
        "enum Flag { Yes, No }
         enum Wrap { Hold((i64 -> i64)) }
         func box(n: i64) -> Flag { Flag::Yes }
         func both(a: i64, b: Flag) -> i64 { of b { Yes => a, No => 0 } }
         func main() -> i64 {
             let f = <0 | box;
             let w = Wrap::Hold({ fn(x: i64) -> i64 { x } });
             of w { Hold(x) => <(4, f) | both }
         }",
    );
    let box_fn = compiled.module.function("box");
    assert!(!box_fn.val_is_pointer);
    assert!(!box_fn.pointer_slots.contains(&0), "{:?}", box_fn.pointer_slots);
    let both = compiled.module.function("both");
    assert!(both.val_is_pointer);
    assert!(both.pointer_slots.contains(&0), "__args {:?}", both.pointer_slots);
    assert!(!both.pointer_slots.contains(&1), "i64 component {:?}", both.pointer_slots);
    assert!(both.pointer_slots.contains(&2), "flag component {:?}", both.pointer_slots);
    let main = compiled.module.function("main");
    assert!(!main.pointer_slots.contains(&0), "unit {:?}", main.pointer_slots);
    assert!(main.pointer_slots.contains(&1), "tagged result {:?}", main.pointer_slots);
    assert!(main.pointer_slots.contains(&3), "delay binding {:?}", main.pointer_slots);
    assert_eq!(link_run(&compiled.object).0, 4);
}

#[test]
fn generic_body_calls_the_monotype() {
    let compiled = pipeline(
        "enum Flag { Yes, No }
         func id<+T>(x: T) -> T { x }
         func apply<+T>(x: T) -> T { <x | id }
         func main() -> i64 {
             let n = <41 | apply;
             let f = <Flag::Yes | apply;
             of f { Yes => n, No => 0 }
         }",
    );
    let apply_i64 = flat(compiled.module.function("apply$i64"));
    assert!(apply_i64.iter().any(|inst| {
        matches!(inst, Inst::CallSlc { symbol, .. } | Inst::Tail { symbol, .. } if symbol == "id$i64")
    }));
    let apply_flag = flat(compiled.module.function("apply$Flag"));
    assert!(apply_flag.iter().any(|inst| {
        matches!(inst, Inst::CallSlc { symbol, .. } | Inst::Tail { symbol, .. } if symbol == "id$Flag")
    }));
    assert!(!compiled.module.functions.iter().any(|func| func.symbol == "id"));
    assert!(!compiled.module.functions.iter().any(|func| func.symbol == "apply"));
    let (status, dir) = link_run(&compiled.object);
    assert_eq!(status, 41);
    let names = symbol_names(&nm_text(&dir.join("p.o")));
    assert!(names.iter().any(|name| name == "id$i64"), "{names:?}");
    assert!(!names.iter().any(|name| name == "id"), "{names:?}");
}

#[test]
fn pointer_tail_does_not_keep_the_scratch() {
    let compiled = pipeline(
        "enum E { A, B, C, D, F }
         func take(p: E) -> i64 { of p { A => 1, B => 2, C => 3, D => 4, F => 5 } }
         func go(n: i64) -> i64 {
             of n {
                 1 => <E::A | take,
                 2 => <E::B | take,
                 3 => <E::C | take,
                 4 => <E::D | take,
                 _ => <E::F | take,
             }
         }
         func main() -> i64 { <5 | go }",
    );
    let go = compiled.module.function("go");
    let mut tails = 0;
    for block in &go.blocks {
        for (index, inst) in block.insts.iter().enumerate() {
            let Inst::Tail { arg_is_pointer: true, .. } = inst else { continue };
            tails += 1;
            let Inst::Store { offset, .. } = block.insts[index - 1] else {
                panic!("tail safepoint has no stored argument: {:?}", block.insts);
            };
            let slot = (offset - FRAME_SLOT0 as i32) / 8;
            assert!(go.pointer_slots.contains(&(slot as u16)), "{slot} {:?}", go.pointer_slots);
        }
    }
    assert_eq!(tails, 5);
    assert_eq!(link_run(&compiled.object).0, 5);
}

/// Frame slots filled by a load of a tagged payload. Those are match occurrence temps.
fn occurrence_slots(func: &Function) -> Vec<u16> {
    let mut slots = Vec::new();
    for block in &func.blocks {
        for pair in block.insts.windows(2) {
            if let (
                Inst::Load { offset: src, .. },
                Inst::Store { base: Dest::Frame, offset: dst, .. },
            ) = (&pair[0], &pair[1])
                && *src == TAGGED_PAYLOAD as i32
            {
                slots.push(((*dst - FRAME_SLOT0 as i32) / 8) as u16);
            }
        }
    }
    slots
}

/// Frame slots loaded back into a heap object at `into` (tag payload or tuple component 0).
fn slots_feeding(func: &Function, into: i32) -> Vec<u16> {
    let mut slots = Vec::new();
    for block in &func.blocks {
        for pair in block.insts.windows(2) {
            if let (
                Inst::Load { base: Dest::Frame, offset: src, .. },
                Inst::Store { offset: dst, .. },
            ) = (&pair[0], &pair[1])
                && *dst == into
            {
                slots.push(((*src - FRAME_SLOT0 as i32) / 8) as u16);
            }
        }
    }
    slots
}

#[test]
fn i64_match_occurrence_is_not_a_pointer() {
    let compiled = pipeline(
        "enum E { A(i64), B }
         enum Flag { Yes, No }
         enum Wrap { Hold(Flag) }
         func id(n: i64) -> i64 { n }
         func binder(e: E) -> i64 { of e { A(n) => <n | id, _ => 0 } }
         func literal(e: E) -> i64 { of e { A(0) => 0, A(n) => <n | id, _ => 0 } }
         func wild(e: E) -> i64 { of e { A(_) => 1, _ => 0 } }
         func held(w: Wrap) -> i64 { of w { Hold(_) => 1, _ => 0 } }
         func main() -> i64 {
             let e = E::A(1);
             <e | binder
         }",
    );
    for name in ["binder", "literal", "wild"] {
        let func = compiled.module.function(name);
        let slots = occurrence_slots(func);
        assert!(!slots.is_empty(), "{name} did not materialize a payload");
        assert!(slots.iter().all(|slot| *slot >= 4), "{name} occurrence {slots:?} is a named slot");
        assert!(
            slots.iter().all(|slot| !func.pointer_slots.contains(slot)),
            "{name} i64 occurrence {slots:?} in {:?}",
            func.pointer_slots
        );
    }
    let held = compiled.module.function("held");
    let slots = occurrence_slots(held);
    assert!(!slots.is_empty(), "Hold(_) did not materialize a payload");
    assert!(slots.iter().all(|slot| *slot >= 4), "held occurrence {slots:?}");
    assert!(
        slots.iter().all(|slot| held.pointer_slots.contains(slot)),
        "Flag wildcard {slots:?} missing from {:?}",
        held.pointer_slots
    );
    assert_eq!(link_run(&compiled.object).0, 1);
}

#[test]
fn match_result_in_a_tag_or_tuple_is_a_root() {
    let compiled = pipeline(
        "enum E { A, B }
         enum Flag { Yes, No }
         enum Wrap { Hold(Flag) }
         func tagged(e: E) -> i64 {
             let w = Wrap::Hold(of e { A => Flag::Yes, _ => Flag::No });
             of w { Hold(Yes) => 1, _ => 2 }
         }
         func paired(e: E) -> i64 {
             let t = (of e { A => Flag::Yes, _ => Flag::No }, 1);
             of t { (Yes, _) => 1, _ => 2 }
         }
         func main() -> i64 {
             let e = E::A;
             <e | tagged
         }",
    );
    let tagged = compiled.module.function("tagged");
    let fed = slots_feeding(tagged, TAGGED_PAYLOAD as i32);
    assert!(!fed.is_empty(), "Hold payload was not saved across allocation: {:?}", flat(tagged));
    assert!(
        fed.iter().all(|slot| tagged.pointer_slots.contains(slot)),
        "tag payload {fed:?} missing from {:?}",
        tagged.pointer_slots
    );
    let map_id = flat(tagged).iter().find_map(|inst| match inst {
        Inst::CallAlloc { tag, map_id, .. }
            if *tag == slc_abi::TAG_TAGGED && *map_id != slc_abi::MAP_EMPTY =>
        {
            Some(*map_id)
        }
        _ => None,
    });
    let map_id = map_id.expect("Hold payload was not entered in a heap map");
    let map = compiled.module.maps.iter().find(|map| map.map_id == map_id).unwrap();
    assert!(map.slots.contains(&1), "{map:?}");
    let paired = compiled.module.function("paired");
    let fed = slots_feeding(paired, 24);
    assert!(!fed.is_empty(), "tuple component was not saved: {:?}", flat(paired));
    assert!(
        fed.iter().all(|slot| paired.pointer_slots.contains(slot)),
        "tuple component {fed:?} missing from {:?}",
        paired.pointer_slots
    );
    assert_eq!(link_run(&compiled.object).0, 1);
}

#[test]
fn fuel_exhaustion_returns_the_diverged_diagnostic() {
    let compiled = pipeline("func main() -> i64 { 1 }");
    let (status, stderr, _) = link_run_fuel(&compiled.object, "1");
    assert_eq!(status, 1);
    assert!(stderr.contains("evaluation diverged (fuel exhausted)"), "{stderr}");
}
