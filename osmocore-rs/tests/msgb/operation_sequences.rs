use osmocore_rs::msgb::{Msgb, MsgbError};

#[derive(Clone, Copy, Debug)]
enum Operation {
    Reserve(usize),
    Put(usize),
    Get(usize),
    Push(usize),
    Pull(usize),
    Trim(i32),
    Reset,
    PutWord,
    PushWord,
}

// A partition model: operations transfer bytes between headroom, live data and
// tailroom. It does not call Msgb or store its data/tail offsets.
struct Model {
    head: Vec<u8>,
    live: Vec<u8>,
    tail: Vec<u8>,
    cb: [u64; 5],
}
impl Model {
    fn new(capacity: usize) -> Self {
        Self {
            head: Vec::new(),
            live: Vec::new(),
            tail: (0..capacity).map(|i| 0xa5 ^ i as u8).collect(),
            cb: [1, 2, 3, 4, 5],
        }
    }
    fn apply(&mut self, op: Operation) -> Result<Option<usize>, MsgbError> {
        match op {
            Operation::Reserve(n) => {
                if !self.live.is_empty() {
                    return Err(MsgbError::ReserveOnNonempty);
                }
                if n > self.tail.len() {
                    return Err(MsgbError::NotEnoughTailroom);
                }
                self.head.extend(self.tail.drain(..n));
                Ok(None)
            }
            Operation::Put(n) => {
                if n > self.tail.len() {
                    return Err(MsgbError::NotEnoughTailroom);
                }
                let offset = self.head.len() + self.live.len();
                self.live.extend(self.tail.drain(..n));
                Ok(Some(offset))
            }
            Operation::Push(n) => {
                if n > self.head.len() {
                    return Err(MsgbError::NotEnoughHeadroom);
                }
                let mut prefix = self.head.split_off(self.head.len() - n);
                prefix.append(&mut self.live);
                self.live = prefix;
                Ok(Some(self.head.len()))
            }
            Operation::Get(n) => {
                if n > self.live.len() {
                    return Err(MsgbError::MessageTooShort);
                }
                let mut removed = self.live.split_off(self.live.len() - n);
                removed.append(&mut self.tail);
                self.tail = removed;
                Ok(Some(self.head.len() + self.live.len()))
            }
            Operation::Pull(n) => {
                if n > self.live.len() {
                    return Err(MsgbError::MessageTooShort);
                }
                self.head.extend(self.live.drain(..n));
                Ok(Some(self.head.len()))
            }
            Operation::Trim(n) => {
                if n < 0 {
                    return Err(MsgbError::NegativeLength);
                }
                let n = n as usize;
                if n > self.live.len() + self.tail.len() {
                    return Err(MsgbError::NotEnoughTailroom);
                }
                if n < self.live.len() {
                    self.apply(Operation::Get(self.live.len() - n))?;
                } else {
                    self.apply(Operation::Put(n - self.live.len()))?;
                }
                Ok(None)
            }
            Operation::Reset => {
                self.head.append(&mut self.live);
                self.head.append(&mut self.tail);
                self.tail = std::mem::take(&mut self.head);
                self.cb = [0; 5];
                Ok(None)
            }
            Operation::PutWord => {
                self.apply(Operation::Put(2))?;
                let end = self.live.len();
                self.live[end - 2..].copy_from_slice(&[0x12, 0x34]);
                Ok(None)
            }
            Operation::PushWord => {
                self.apply(Operation::Push(2))?;
                self.live[..2].copy_from_slice(&[0x56, 0x78]);
                Ok(None)
            }
        }
    }
    fn check(&self, msg: &Msgb) {
        let backing: Vec<_> = self
            .head
            .iter()
            .chain(&self.live)
            .chain(&self.tail)
            .copied()
            .collect();
        assert_eq!(msg.backing(), backing);
        assert_eq!(msg.data(), self.live);
        assert_eq!(msg.data_offset(), self.head.len());
        assert_eq!(msg.tail_offset(), self.head.len() + self.live.len());
        assert_eq!(usize::from(msg.len()), self.live.len());
        assert_eq!(msg.headroom(), self.head.len() as i32);
        assert_eq!(msg.tailroom(), self.tail.len() as i32);
        assert_eq!(msg.cb, self.cb);
        assert!(msg.check_invariants());
    }
}
fn apply(msg: &mut Msgb, op: Operation) -> Result<Option<usize>, MsgbError> {
    match op {
        Operation::Reserve(n) => msg.reserve(n).map(|()| None),
        Operation::Put(n) => msg.put(n).map(Some),
        Operation::Get(n) => msg.get(n).map(Some),
        Operation::Push(n) => msg.push(n).map(Some),
        Operation::Pull(n) => msg.pull(n).map(Some),
        Operation::Trim(n) => msg.trim(n).map(|()| None),
        Operation::Reset => {
            msg.reset();
            Ok(None)
        }
        Operation::PutWord => msg.put_u16(0x1234).map(|()| None),
        Operation::PushWord => msg.push_u16(0x5678).map(|()| None),
    }
}
#[test]
fn every_three_operation_sequence_matches_an_independent_partition_model() {
    for capacity in [0, 1, 2, 4, 8] {
        let operations = [
            Operation::Reserve(0),
            Operation::Reserve(2),
            Operation::Reserve(capacity + 1),
            Operation::Put(0),
            Operation::Put(1),
            Operation::Put(capacity + 1),
            Operation::Get(1),
            Operation::Push(1),
            Operation::Pull(1),
            Operation::Trim(-1),
            Operation::Trim(0),
            Operation::Trim(capacity as i32),
            Operation::Reset,
            Operation::PutWord,
            Operation::PushWord,
        ];
        for &a in &operations {
            for &b in &operations {
                for &c in &operations {
                    let mut model = Model::new(capacity);
                    let mut msg = Msgb::new(capacity).unwrap();
                    msg.backing_mut().copy_from_slice(&model.tail);
                    msg.cb = model.cb;
                    for op in [a, b, c] {
                        assert_eq!(
                            apply(&mut msg, op),
                            model.apply(op),
                            "capacity {capacity}, sequence {a:?}/{b:?}/{c:?}"
                        );
                        model.check(&msg);
                    }
                }
            }
        }
    }
}

#[test]
fn repeated_copy_resize_and_reset_preserve_ownership_and_l1() {
    let mut msg = Msgb::with_headroom(32, 4).unwrap();
    for round in 0..64u8 {
        msg.put_u16(u16::from(round)).unwrap();
        msg.set_layer_offset(1, Some(0)).unwrap();
        msg.set_layer_offset(2, Some(msg.data_offset())).unwrap();
        msg.cb = [u64::from(round); 5];
        let snapshot = msg.backing().to_vec();
        let mut copy = msg.copy_resize(40).unwrap();
        assert_eq!(copy.data(), msg.data());
        assert_eq!(copy.layer_offsets(), msg.layer_offsets());
        assert_eq!(copy.cb, [0; 5]);
        copy.data_mut()[0] ^= 0xff;
        assert_eq!(msg.backing(), snapshot);
        assert_ne!(copy.data(), msg.data());
        msg.reset();
        assert!(msg.is_empty());
        assert_eq!(msg.layer_offsets(), [Some(0), None, None, None]);
        assert_eq!(msg.cb, [0; 5]);
        assert_eq!(msg.backing(), snapshot);
        msg.reserve(4).unwrap();
        assert!(msg.check_invariants());
    }
}

#[test]
fn complete_owned_hexdumps_preserve_bytes_beyond_the_c_segment_limit() {
    let mut msg = Msgb::new(4096).unwrap();
    msg.put(4096).unwrap();
    msg.data_mut().fill(0xa5);
    assert_eq!(msg.hexdump(), "a5 ".repeat(4096));
    assert_eq!(msg.data(), vec![0xa5; 4096]);
    assert!(msg.check_invariants());
}
