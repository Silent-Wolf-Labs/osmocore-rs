// SPDX-License-Identifier: GPL-3.0-or-later

use super::Msgb;
pub type MsgbQueue = std::collections::VecDeque<Msgb>;

pub fn msgb_enqueue(queue: &mut MsgbQueue, msg: Msgb) {
    queue.push_back(msg);
}
pub fn msgb_dequeue(queue: &mut MsgbQueue) -> Option<Msgb> {
    queue.pop_front()
}
/// Drop all queued messages. External counters are not changed, matching C.
pub fn msgb_queue_free(queue: &mut MsgbQueue) {
    queue.clear();
}
/// C's caller-owned unsigned counter wraps independently of queue length.
pub fn msgb_enqueue_count(queue: &mut MsgbQueue, msg: Msgb, count: &mut u32) {
    msgb_enqueue(queue, msg);
    *count = count.wrapping_add(1);
}
pub fn msgb_dequeue_count(queue: &mut MsgbQueue, count: &mut u32) -> Option<Msgb> {
    let msg = msgb_dequeue(queue);
    if msg.is_some() {
        *count = count.wrapping_sub(1);
    }
    msg
}
