//! Queue of generation tasks waiting for a running slot.

use std::{collections::BTreeMap, mem};

use super::{Arc, ChunkGenerationTask, ChunkMap, GenerationTaskPriority};

/// Generation tasks waiting for a running slot, in dispatch order.
///
/// Dispatch order is (priority from the holder's current ticket levels, queue
/// order): the same order as stable-sorting the whole queue at every refill.
/// Priorities are cached in the keys and recomputed by [`Self::reprioritize`]
/// only after a ticket level changed, so a refill pops `k` tasks in
/// `O(k log n)` instead of re-sorting every pending task under the lock.
pub struct PendingGenerationTasks {
    queue: BTreeMap<(GenerationTaskPriority, u64), Arc<ChunkGenerationTask>>,
    next_sequence: u64,
}

impl PendingGenerationTasks {
    pub(super) const fn new() -> Self {
        Self {
            queue: BTreeMap::new(),
            next_sequence: 0,
        }
    }

    /// Queues a task behind every pending task of the same priority.
    pub(super) fn push(&mut self, task: Arc<ChunkGenerationTask>) {
        let priority = ChunkMap::generation_task_priority(&task);
        self.insert(priority, task);
    }

    fn insert(&mut self, priority: GenerationTaskPriority, task: Arc<ChunkGenerationTask>) {
        self.queue.insert((priority, self.next_sequence), task);
        self.next_sequence += 1;
    }

    /// Drops cancelled tasks and recomputes every priority from current ticket
    /// levels, keeping queue order between tasks of equal priority.
    pub(super) fn reprioritize(&mut self) {
        let mut tasks = mem::take(&mut self.queue)
            .into_values()
            .filter(|task| !task.is_cancelled())
            .collect::<Vec<_>>();
        tasks.sort_by_cached_key(|task| ChunkMap::generation_task_priority(task));
        for task in tasks {
            let priority = ChunkMap::generation_task_priority(&task);
            self.insert(priority, task);
        }
    }

    /// Removes up to `count` uncancelled tasks in dispatch order.
    pub(super) fn pop_runnable(&mut self, count: usize) -> Vec<Arc<ChunkGenerationTask>> {
        let mut tasks = Vec::with_capacity(count.min(self.queue.len()));
        while tasks.len() < count {
            let Some((_, task)) = self.queue.pop_first() else {
                break;
            };
            if !task.is_cancelled() {
                tasks.push(task);
            }
        }
        tasks
    }

    /// Removes every queued task.
    pub(super) fn take_all(&mut self) -> Vec<Arc<ChunkGenerationTask>> {
        mem::take(&mut self.queue).into_values().collect()
    }

    /// Number of queued tasks, including cancelled ones not yet dropped.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether no task is queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// The next task in dispatch order.
    #[must_use]
    pub fn first(&self) -> Option<&Arc<ChunkGenerationTask>> {
        self.queue.first_key_value().map(|(_, task)| task)
    }
}
