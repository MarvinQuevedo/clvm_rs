use clvmr::allocator::{Allocator, NodePtr};
use clvmr::chia_dialect::{ChiaDialect, ENABLE_BLS_OPS_OUTSIDE_GUARD};
use clvmr::run_program::run_program;
use linreg::linear_regression_of;
use std::time::Instant;

#[derive(Clone, Copy)]
enum OpArgs {
    None,
    SingleArg(NodePtr),
    TwoArgs(NodePtr, NodePtr),
}

// builds calls in the form:
// (<op> arg arg ...)
// where "num" specifies the number of arguments
// if arg is a pair, it's unwrapped into two arguments
fn build_call(
    a: &mut Allocator,
    op: u32,
    arg: OpArgs,
    num: i32,
    extra: Option<NodePtr>,
) -> NodePtr {
    let mut args = a.null();
    for _i in 0..num {
        match arg {
            OpArgs::None => {}
            OpArgs::SingleArg(a1) => {
                args = a.new_pair(a1, args).unwrap();
            }
            OpArgs::TwoArgs(first, second) => {
                args = a.new_pair(second, args).unwrap();
                args = a.new_pair(first, args).unwrap();
            }
        }
    }
    if let Some(extra) = extra {
        args = a.new_pair(extra, args).unwrap();
    }
    let op_code = a.new_number(op.into()).unwrap();
    a.new_pair(op_code, args).unwrap()
}

// builds calls in the form:
// (<op> [extra] (<op> [extra] (<op> [extra] arg)))
// "extra" is optional, "num" specifies the nesting depth
fn build_nested_call(
    a: &mut Allocator,
    op: u32,
    mut arg: OpArgs,
    num: i32,
    extra: Option<NodePtr>,
) -> NodePtr {
    let op_code = a.new_number(op.into()).unwrap();
    for _i in 0..num {
        let mut args = a.null();
        match arg {
            OpArgs::None => {}
            OpArgs::SingleArg(a1) => {
                args = a.new_pair(a1, args).unwrap();
            }
            OpArgs::TwoArgs(first, second) => {
                args = a.new_pair(second, args).unwrap();
                args = a.new_pair(first, args).unwrap();
            }
        }
        if let Some(extra) = extra {
            args = a.new_pair(extra, args).unwrap();
        }
        args = a.new_pair(op_code, args).unwrap();
        arg = OpArgs::SingleArg(args);
    }
    match arg {
        OpArgs::SingleArg(ret) => ret,
        _ => {
            panic!("unexpected");
        }
    }
}

fn quote(a: &mut Allocator, v: NodePtr) -> NodePtr {
    a.new_pair(a.one(), v).unwrap()
}

// returns the time per byte
// measures run-time of many calls
fn time_per_byte(a: &mut Allocator, op: u32, extra: Option<NodePtr>) -> f64 {
    let checkpoint = a.checkpoint();
    let mut samples = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_BLS_OPS_OUTSIDE_GUARD);

    let atom = vec![0; 10000000];
    for i in (0..10000000).step_by(1000) {
        let mut args = a.null();
        let arg = a.new_atom(&atom[0..i]).unwrap();
        let arg = quote(a, arg);
        args = a.new_pair(arg, args).unwrap();

        if let Some(extra) = extra {
            args = a.new_pair(extra, args).unwrap();
        }

        let op_code = a.new_number(op.into()).unwrap();
        let call = a.new_pair(op_code, args).unwrap();
        let start = Instant::now();
        let _ = run_program(a, &dialect, call, a.null(), 11000000000).unwrap();
        let duration = start.elapsed();
        samples.push((i as f64, duration.as_nanos() as f64));
        a.restore_checkpoint(&checkpoint);
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

// returns the time per argument
// measures the run-time of many calls with varying number of arguments, to
// establish how much time each additional argument contributes
fn time_per_arg(a: &mut Allocator, op: u32, arg: OpArgs, extra: Option<NodePtr>) -> f64 {
    let checkpoint = a.checkpoint();
    let mut samples = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_BLS_OPS_OUTSIDE_GUARD);

    for _k in 0..3 {
        for i in 0..100 {
            let call = build_call(a, op, arg, i, extra);
            let start = Instant::now();
            let _ = run_program(a, &dialect, call, a.null(), 11000000000);
            let duration = start.elapsed();
            samples.push((i as f64, duration.as_nanos() as f64));

            a.restore_checkpoint(&checkpoint);
        }
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

// measure run-time of many *nested* calls, to establish how much longer it
// takes, approximately, for each additional nesting. The per_arg_time is
// subtracted to get the base cost
fn base_call_time(
    a: &mut Allocator,
    op: u32,
    per_arg_time: f64,
    arg: OpArgs,
    extra: Option<NodePtr>,
) -> f64 {
    let checkpoint = a.checkpoint();
    let mut samples = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_BLS_OPS_OUTSIDE_GUARD);

    for _k in 0..3 {
        for i in 1..100 {
            a.restore_checkpoint(&checkpoint);
            let call = build_nested_call(a, op, arg, i, extra);
            let start = Instant::now();
            let _ = run_program(a, &dialect, call, a.null(), 11000000000);
            let duration = start.elapsed();
            let duration = (duration.as_nanos() as f64) - (per_arg_time * i as f64);
            samples.push((i as f64, duration));

            a.restore_checkpoint(&checkpoint);
        }
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

fn base_call_time_no_nest(
    a: &mut Allocator,
    op: u32,
    arg: OpArgs,
    per_arg_time: f64,
    extra: Option<NodePtr>,
) -> f64 {
    let checkpoint = a.checkpoint();
    let dialect = ChiaDialect::new(ENABLE_BLS_OPS_OUTSIDE_GUARD);

    let mut total_time: u64 = 0;
    let mut num_samples = 0;

    for _k in 0..3 {
        for _i in 1..100 {
            a.restore_checkpoint(&checkpoint);
            let call = build_call(a, op, arg, 1, extra);
            let start = Instant::now();
            let _ = run_program(a, &dialect, call, a.null(), 11000000000);
            let duration = start.elapsed();
            total_time += duration.as_nanos() as u64;
            num_samples += 1;

            a.restore_checkpoint(&checkpoint);
        }
    }

    (total_time as f64 - per_arg_time * num_samples as f64) / num_samples as f64
}

enum Mode {
    Nesting,
    Unary,
    FreeBytes,
    MultiArg,
}

struct Operator {
    opcode: u32,
    name: &'static str,
    arg: OpArgs,
    extra: Option<NodePtr>,
    mode: Mode,
}

pub fn main() {
    let mut a = Allocator::new();

    let g1 = a.new_atom(&hex::decode("97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb").unwrap()).unwrap();
    let g2 = a.new_atom(&hex::decode("93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8").unwrap()).unwrap();

    let g1 = quote(&mut a, g1);
    let g2 = quote(&mut a, g2);

    let ops: [Operator; 12] = [
        Operator {
            opcode: 29,
            name: "point_add",
            arg: OpArgs::SingleArg(g1),
            extra: None,
            mode: Mode::Nesting,
        },
        Operator {
            opcode: 49,
            name: "g1_subtract",
            arg: OpArgs::SingleArg(g1),
            extra: None,
            mode: Mode::Nesting,
        },
        Operator {
            opcode: 50,
            name: "g1_multiply",
            arg: OpArgs::None,
            extra: Some(g1),
            mode: Mode::FreeBytes,
        },
        Operator {
            opcode: 51,
            name: "g1_negate",
            arg: OpArgs::SingleArg(g1),
            extra: None,
            mode: Mode::Unary,
        },
        Operator {
            opcode: 52,
            name: "g2_add",
            arg: OpArgs::SingleArg(g2),
            extra: None,
            mode: Mode::Nesting,
        },
        Operator {
            opcode: 53,
            name: "g2_subtract",
            arg: OpArgs::SingleArg(g2),
            extra: None,
            mode: Mode::Nesting,
        },
        Operator {
            opcode: 54,
            name: "g2_multiply",
            arg: OpArgs::None,
            extra: Some(g2),
            mode: Mode::FreeBytes,
        },
        Operator {
            opcode: 55,
            name: "g2_negate",
            arg: OpArgs::SingleArg(g2),
            extra: None,
            mode: Mode::Unary,
        },
        Operator {
            opcode: 56,
            name: "g1_map",
            arg: OpArgs::None,
            extra: None,
            mode: Mode::FreeBytes,
        },
        Operator {
            opcode: 57,
            name: "g2_map",
            arg: OpArgs::None,
            extra: None,
            mode: Mode::FreeBytes,
        },
        Operator {
            opcode: 58,
            name: "bls_pairing_identity",
            arg: OpArgs::TwoArgs(g1, g2),
            extra: None,
            mode: Mode::MultiArg,
        },
        Operator {
            opcode: 59,
            name: "bls_verify",
            arg: OpArgs::TwoArgs(g1, g2),
            extra: Some(g2),
            mode: Mode::MultiArg,
        },
    ];

    // this "magic" scaling depends on the computer you run the tests on.
    // It's calibrated against the timing of point_add, which has a cost
    let cost_scale = ((101094.0 / 39000.0) + (1343980.0 / 131000.0)) / 2.0;
    let base_cost_scale = 101094.0 / 42500.0;
    let arg_cost_scale = 1343980.0 / 129000.0;
    println!("cost scale: {cost_scale}");
    println!("base cost scale: {base_cost_scale}");
    println!("arg cost scale: {arg_cost_scale}");

    for op in &ops {
        println!("opcode: {} ({})", op.name, op.opcode);
        match op.mode {
            Mode::Nesting => {
                let time_per_arg = time_per_arg(&mut a, op.opcode, op.arg, op.extra);
                let base_call_time =
                    base_call_time(&mut a, op.opcode, time_per_arg, op.arg, op.extra);
                println!("   time: base: {base_call_time:.2}ns per-arg: {time_per_arg:.2}ns");
                println!(
                    "   cost: base: {:.0} per-arg: {:.0}",
                    base_call_time * base_cost_scale,
                    time_per_arg * arg_cost_scale
                );
            }
            Mode::Unary => {
                let base_call_time = base_call_time(&mut a, op.opcode, 0.0, op.arg, op.extra);
                println!("   time: base: {base_call_time:.2}ns");
                println!("   cost: base: {:.0}", base_call_time * cost_scale);
            }
            Mode::FreeBytes => {
                let time_per_byte = time_per_byte(&mut a, op.opcode, op.extra);
                let base_call_time =
                    base_call_time(&mut a, op.opcode, 0.0, OpArgs::SingleArg(g1), op.extra);
                println!("   time: base: {base_call_time:.2}ns per-byte: {time_per_byte:.2}ns");
                println!(
                    "   cost: base: {:.0} per-byte: {:.0}",
                    base_call_time * base_cost_scale,
                    time_per_byte * cost_scale
                );
            }
            Mode::MultiArg => {
                let time_per_arg = time_per_arg(&mut a, op.opcode, op.arg, op.extra);
                let base_call_time =
                    base_call_time_no_nest(&mut a, op.opcode, op.arg, time_per_arg, op.extra);
                println!("   time: base: {base_call_time:.2}ns per-arg: {time_per_arg:.2}ns");
                println!(
                    "   cost: base: {:.0} per-arg: {:.0}",
                    base_call_time * cost_scale,
                    time_per_arg * cost_scale
                );
            }
        }
    }
}
