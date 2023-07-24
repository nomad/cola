use cola::{CrdtEdit, Length};
use criterion::measurement::WallTime;
use criterion::{
    criterion_group,
    criterion_main,
    BenchmarkGroup,
    Criterion,
    Throughput,
};
use traces::{ConcurrentTraceInfos, Crdt, Edit};

struct Replica(cola::Replica);

impl Crdt for Replica {
    type EDIT = CrdtEdit;

    fn from_str(s: &str) -> Self {
        Self(cola::Replica::new(rand::random::<u64>(), s.len() as Length))
    }

    fn fork(&self) -> Self {
        Self(self.0.fork(rand::random::<u64>()))
    }

    fn local_insert(&mut self, offset: usize, text: &str) -> Self::EDIT {
        self.0.inserted(offset as Length, text.len() as Length)
    }

    fn local_delete(&mut self, start: usize, end: usize) -> Self::EDIT {
        self.0.deleted(start as Length..end as Length)
    }

    fn remote_merge(&mut self, remote_edit: &Self::EDIT) {
        self.0.merge(remote_edit);
    }
}

fn bench<const N: usize>(
    group: &mut BenchmarkGroup<WallTime>,
    trace: ConcurrentTraceInfos<N, Replica>,
    name: &str,
) {
    let ConcurrentTraceInfos { trace, mut peers, .. } = trace;

    group.throughput(Throughput::Elements(trace.num_edits() as u64));

    group.bench_function(name, |b| {
        b.iter(|| {
            for edit in trace.edits() {
                match edit {
                    Edit::Insertion(idx, offset, text) => {
                        peers[*idx].local_insert(*offset, text.as_str());
                    },
                    Edit::Deletion(idx, start, end) => {
                        peers[*idx].local_delete(*start, *end);
                    },
                    Edit::Merge(idx, edit) => {
                        peers[*idx].remote_merge(edit);
                    },
                }
            }
        })
    });
}

fn friends_forever(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent");
    bench(&mut group, traces::friends_forever(), "friends_forever");
}

criterion_group!(benches, friends_forever);
criterion_main!(benches);
