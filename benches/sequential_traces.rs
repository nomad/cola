use cola::{Length, Replica};
use criterion::measurement::WallTime;
use criterion::{
    criterion_group,
    criterion_main,
    BenchmarkGroup,
    Criterion,
    Throughput,
};
use traces::{TestData, TestPatch};

fn bench(group: &mut BenchmarkGroup<WallTime>, trace: &TestData, name: &str) {
    let trace = trace.chars_to_bytes();

    group.throughput(Throughput::Elements(trace.len() as u64));

    group.bench_function(name, |b| {
        b.iter(|| {
            let mut replica =
                Replica::new(trace.start_content.len() as Length);

            for txn in &trace.txns {
                for &TestPatch(pos, del, ref ins) in &txn.patches {
                    let start = pos as Length;
                    let end = start + del as Length;
                    replica.deleted(start..end);
                    replica.inserted(start, ins.len() as Length);
                }
            }
        })
    });
}

fn automerge(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench(&mut group, &traces::automerge(), "automerge");
}

fn rustcode(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench(&mut group, &traces::rustcode(), "rustcode");
}

fn seph_blog(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench(&mut group, &traces::seph_blog(), "seph_blog");
}

fn sveltecomponent(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench(&mut group, &traces::sveltecomponent(), "sveltecomponent");
}

criterion_group!(benches, automerge, rustcode, seph_blog, sveltecomponent);
criterion_main!(benches);
