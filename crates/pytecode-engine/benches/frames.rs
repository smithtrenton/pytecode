//! Isolated frame analysis: javac fixtures plus wide switch worklists.
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use pytecode_engine::analysis::recompute_frames;
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::model::{
    ClassModel, CodeItem, CodeModel, DebugInfoState, Label, TableSwitchInsn, VarInsn,
};
use std::hint::black_box;

fn wide_switch(cases: usize) -> CodeModel {
    let labels: Vec<_> = (0..cases).map(|_| Label::new()).collect();
    let mut code = CodeModel::new(1, 1, DebugInfoState::Fresh);
    code.instructions = vec![
        CodeItem::Var(VarInsn {
            opcode: 0x15,
            slot: 0,
        }),
        CodeItem::TableSwitch(TableSwitchInsn {
            default_target: labels[0].clone(),
            low: 0,
            high: cases as i32 - 1,
            targets: labels.clone(),
        }),
    ];
    for label in labels {
        code.instructions.push(CodeItem::Label(label));
        code.instructions.extend(
            pytecode_engine::parse_instructions(&[0x03, 0xac])
                .unwrap()
                .into_iter()
                .map(CodeItem::Raw),
        );
    }
    code
}

fn bench_frames(c: &mut Criterion) {
    let mut group = c.benchmark_group("frames");
    for cases in [32, 256, 2048] {
        let code = wide_switch(cases);
        group.bench_with_input(BenchmarkId::new("switch", cases), &code, |b, code| {
            b.iter(|| {
                black_box(
                    recompute_frames(
                        code,
                        "Benchmark",
                        "run",
                        "(I)I",
                        MethodAccessFlags::STATIC,
                        None,
                    )
                    .unwrap(),
                )
            });
        });
    }
    let models = [
        include_bytes!("../fixtures/classes/InstructionShowcase/InstructionShowcase.class")
            .as_slice(),
        include_bytes!("../fixtures/classes/TryCatchExample/TryCatchExample.class").as_slice(),
        include_bytes!("../fixtures/classes/SwitchExpressions/SwitchExpressions.class").as_slice(),
    ]
    .map(|bytes| ClassModel::from_bytes(bytes).unwrap());
    group.bench_function("javac-fixtures", |b| {
        b.iter(|| {
            for model in &models {
                for method in &model.methods {
                    if let Some(code) = &method.code {
                        black_box(
                            recompute_frames(
                                code,
                                &model.name,
                                &method.name,
                                &method.descriptor,
                                method.access_flags,
                                None,
                            )
                            .unwrap(),
                        );
                    }
                }
            }
        });
    });
    group.finish();
}

criterion_group!(benches, bench_frames);
criterion_main!(benches);
