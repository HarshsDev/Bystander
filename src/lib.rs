use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::atomic::Ordering::Relaxed;
pub struct ContentionMeasure(usize);
impl ContentionMeasure {
    pub fn detected(&mut self) {
        self.0 += 1;
    }
}

pub trait NormalizedLockFree {
    type Input;
    type Output;
    type CasDescriptor;
    fn prepare(&self, op: &Self::Input) -> [Self::CasDescriptor; 1];
    fn execute(
        &self,
        cases: &[Self::CasDescriptor],
        i: usize,
        contention: ContentionMeasure,
    ) -> Result<(), usize>;
    fn cleanup(&self);
}

struct WaitFreeSimulator<LF: NormalizedLockFree> {
    algorithm: LF,
    help: HelpQueue,
}

pub struct Help {
    completed: AtomicBool,
    at: AtomicUsize,
}

pub struct HelpQueue;

impl HelpQueue {
    pub fn add(&self, help: *const Help) {}
    pub fn peek(&self) -> Option<*const Help> {
        todo!();
    }

    pub fn try_remove_front(&self, completed: *const Help) {}
}

impl<LF: NormalizedLockFree> WaitFreeSimulator<LF> {
    pub fn help(&self) {
        if let Some(help) = self.help.peek() {}
    }
    pub fn run(&self, op: LF::Input) -> LF::Output {
        if false {
            self.help();
        }

        let mut contention = ContentionMeasure(0);
        let cas = self.algorithm.prepare(&op);
        match self.algorithm.execute(&cas[..], 0, contention) {
            Ok(()) => {
                self.algorithm.cleanup();
            }
            Err(i) => {
                let help = Help {
                    completed: AtomicBool::new(false),
                    at: AtomicUsize::new(i)
                };
                self.help.add(&help);
                while !help.completed.load(Relaxed) {
                    self.help();
                }
            }
        }

        todo!()
    }
}
// pub struct WaitFreeLinkedlist <T> {
//     simulator: WaitFreeSimulator<LockFreeLinkedList<T>>,
// }

// struct LockFreeLinkedList<T> {
//     t:T,
// }

// impl<T> NormalizedLockFree for LockFreeLinkedList<T> {

// }
// impl <T> WaitFreeLinkedlist<T> {
//     pub fn push_front(&self, t:T) {
//         let i = self.simulator.run()
//     }
// }
