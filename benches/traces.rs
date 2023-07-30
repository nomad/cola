use cola::Replica;
use criterion::measurement::WallTime;
use criterion::{
    criterion_group,
    criterion_main,
    BenchmarkGroup,
    BenchmarkId,
    Criterion,
    Throughput,
};
use traces::SequentialTrace;

fn bench_upstream(
    group: &mut BenchmarkGroup<WallTime>,
    trace: &SequentialTrace,
    trace_name: &str,
) {
    let trace = trace.chars_to_bytes();

    group.throughput(Throughput::Elements(trace.num_edits() as u64));

    group.bench_function(BenchmarkId::new("upstream", trace_name), |b| {
        b.iter(|| {
            let mut replica = Replica::new(1, trace.start_content().len());

            for (start, end, text) in trace.edits() {
                replica.deleted(start..end);
                replica.inserted(start, text.len());
            }

            assert_eq!(replica.len(), trace.end_content().len());
        })
    });
}

fn bench_downstream(
    group: &mut BenchmarkGroup<WallTime>,
    trace: &SequentialTrace,
    trace_name: &str,
) {
    enum Edit {
        Insertion(cola::Insertion),
        Deletion(cola::Deletion),
    }

    let trace = trace.chars_to_bytes();

    let mut upstream = Replica::new(1, trace.start_content().len());

    let edits = trace
        .edits()
        .flat_map(|(start, end, text)| {
            let mut edits = Vec::new();
            if end > start {
                edits.push(Edit::Deletion(upstream.deleted(start..end)));
            }
            if !text.is_empty() {
                edits.push(Edit::Insertion(
                    upstream.inserted(start, text.len()),
                ));
            }
            edits
        })
        .collect::<Vec<_>>();

    group.throughput(Throughput::Elements(edits.len() as u64));

    group.bench_function(BenchmarkId::new("downstream", trace_name), |b| {
        b.iter(|| {
            let upstream = Replica::new(1, trace.start_content().len());

            let mut downstream = upstream.fork(2);

            for edit in &edits {
                match edit {
                    Edit::Insertion(insertion) => {
                        downstream.integrate_insertion(insertion);
                    },
                    Edit::Deletion(deletion) => {
                        downstream.integrate_deletion(deletion);
                    },
                };
            }

            assert_eq!(downstream.len(), trace.end_content().len());
        })
    });
}

fn upstream_automerge(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench_upstream(&mut group, &traces::automerge(), "automerge");
}

fn upstream_rustcode(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench_upstream(&mut group, &traces::rustcode(), "rustcode");
}

fn upstream_seph_blog(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench_upstream(&mut group, &traces::seph_blog(), "seph_blog");
}

fn upstream_sveltecomponent(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench_upstream(&mut group, &traces::sveltecomponent(), "sveltecomponent");
}

fn downstream_automerge(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench_downstream(&mut group, &traces::automerge(), "automerge");
}

fn downstream_rustcode(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench_downstream(&mut group, &traces::rustcode(), "rustcode");
}

fn downstream_seph_blog(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench_downstream(&mut group, &traces::seph_blog(), "seph_blog");
}

fn downstream_sveltecomponent(c: &mut Criterion) {
    let mut group = c.benchmark_group("traces");
    bench_downstream(
        &mut group,
        &traces::sveltecomponent(),
        "sveltecomponent",
    );
}

criterion_group!(
    benches,
    upstream_automerge,
    upstream_rustcode,
    upstream_seph_blog,
    upstream_sveltecomponent,
    downstream_automerge,
    downstream_rustcode,
    downstream_seph_blog,
    downstream_sveltecomponent,
);
criterion_main!(benches);
