use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use slc_abi::{TAGGED_LABEL, TAGGED_PAYLOAD};

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
    crate::compile(&defs, &dispatch.specializations).unwrap_or_else(|err| panic!("{err}"))
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
    let dir = std::env::temp_dir().join(format!(
        "slc-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let object_path = dir.join("p.o");
    std::fs::write(&object_path, object).unwrap();
    std::fs::write(dir.join("main.c"), DRIVER).unwrap();
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
    let ran = Command::new(&exe).status().unwrap();
    (ran.code().unwrap_or(127), dir)
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
            slot_count: 12,
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
