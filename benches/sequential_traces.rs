use cola::{Length, Replica};
use criterion::measurement::WallTime;
use criterion::{
    criterion_group,
    criterion_main,
    BenchmarkGroup,
    Criterion,
    Throughput,
};
use traces::SequentialTrace;

fn bench(
    group: &mut BenchmarkGroup<WallTime>,
    trace: &SequentialTrace,
    name: &str,
) {
    let trace = trace.chars_to_bytes();

    group.throughput(Throughput::Elements(trace.num_edits() as u64));

    group.bench_function(name, |b| {
        b.iter(|| {
            let mut replica =
                Replica::new(0, trace.start_content().len() as Length);

            for (start, end, text) in trace.edits() {
                let start = start as Length;
                replica.deleted(start..end as Length);
                replica.inserted(start, text.len() as Length);
            }
        })
    });
}

fn automerge(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential");
    bench(&mut group, &traces::automerge(), "automerge");
}

fn rustcode(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential");
    bench(&mut group, &traces::rustcode(), "rustcode");
}

fn seph_blog(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential");
    bench(&mut group, &traces::seph_blog(), "seph_blog");
}

fn sveltecomponent(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential");
    bench(&mut group, &traces::sveltecomponent(), "sveltecomponent");
}

criterion_group!(benches, automerge, rustcode, seph_blog, sveltecomponent);
criterion_main!(benches);
