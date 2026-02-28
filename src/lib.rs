// use std::clone;
use std::marker::PhantomData;
use std::ops::Index;
// use std::process::Output;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering::{self};

const CONTENTION_THRESHOLD: usize = 2;
const RETRY_THRESHOLD: usize = 2;

pub struct ContentionMeasure(usize);

pub struct Contention;

impl ContentionMeasure {
    pub fn detected(&mut self) -> Result<(), Contention> {
        self.0 += 1;
        if self.0 < CONTENTION_THRESHOLD {
            Ok(())
        } else {
            Err(Contention)
        }
    }

    pub fn use_slow_path(&self) -> bool {
        self.0 > CONTENTION_THRESHOLD
    }
}

pub trait CasDescriptor {
    fn execute(&self) -> Result<(), ()>;
}

pub trait CasDescriptors<D>: Index<usize, Output = D>
where
    D: CasDescriptor,
{
    fn len(&self) -> usize {
        todo!()
    }
}

pub trait NormalizedLockFree {
    type Input: Clone;
    type Output: Clone;
    type cas: CasDescriptor;
    type cases: CasDescriptors<Self::cas> + Clone;
    fn generate(
        &self,
        op: &Self::Input,
        contention: &mut ContentionMeasure,
    ) -> Result<Self::Output, Contention>;
    // fn execute(
    //     &self,
    //     cases:Self::Descriptor,
    //     i: usize,
    //     contention: ContentionMeasure,
    // ) -> Result<(), usize>;
    fn wrap_up(
        &self,
        executed: Result<(), usize>,
        performed: &Self::cases,
        contention: &mut ContentionMeasure,
    ) -> Result<Option<Self::Output>, Contention>;
}

struct WaitFreeSimulator<LF: NormalizedLockFree> {
    algorithm: LF,
    help: HelpQueue<LF>,
}

#[derive(Clone)]
enum OperationState<LF: NormalizedLockFree> {
    PreCas,
    ExecuteCas(LF::cases),
    PostCas(LF::cases, Result<(), usize>),
    Completed(LF::Output),
}

impl<LF: NormalizedLockFree> OperationState<LF> {
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(..))
    }
}

struct OperationRecordBox<LF: NormalizedLockFree> {
    val: AtomicPtr<OperationRecord<LF>>,
}
struct OperationRecord<LF: NormalizedLockFree> {
    owner: std::thread::ThreadId,
    input: LF::Input,
    state: OperationState<LF>,
    //  cas_list: LF::cases, // completed: bool,
    // at: usize,
}

impl<LF: NormalizedLockFree> Clone for OperationRecord<LF>
where
    LF::Input: Clone,
    LF::Output: Clone,
    LF::cases: Clone,
    OperationState<LF>: Clone,
{
    fn clone(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            input: self.input.clone(),
            state: self.state.clone(), // cas_list: self.cas_list.clone(),
        }
    }
}

struct HelpQueue<LF> {
    _o: PhantomData<LF>,
}

impl<LF: NormalizedLockFree> HelpQueue<LF> {
    fn enqueue(&self, help: *const OperationRecordBox<LF>) {
        let _ = help;
        todo!()
    }
    fn peek(&self) -> Option<*const OperationRecordBox<LF>> {
        todo!();
    }

    fn try_remove_front(&self, front: *const OperationRecordBox<LF>) -> Result<(), ()> {
        let _ = front;
        Err(())
    }
}

impl<LF: NormalizedLockFree> WaitFreeSimulator<LF>
// where
//     OperationRecord<LF>: Clone,
{
    fn cas_executor(
        &self,
        descriptors: &LF::cases,
        contention: &mut ContentionMeasure,
    ) -> Result<(), usize> {
        let len = descriptors.len();
        for i in 0..len {
            if descriptors[i].execute().is_err() {
                contention.detected();
                return Err(i);
            }
        }
        Ok(())
        // todo!()
    }

    //  pub fn execute() -> Result<(),()> {
    //     todo!()
    //  }
    fn help_op(&self, orb: &OperationRecordBox<LF>) {
        loop {
            let or = unsafe { &*orb.val.load(Ordering::SeqCst) };
            let updated_or = match &or.state {
                OperationState::Completed(..) => {
                    let _ = self.help.try_remove_front(orb);
                    return;
                },
                OperationState::PreCas => {
                    //  let mut updated_or = Box::new(or.clone());
                    let cas_list = match self
                        .algorithm
                        .generate(&or.input, &mut ContentionMeasure(0))
                    {
                        Ok(cas_list) => cas_list,
                        Err(Contention) => {
                            continue;
                        }
                    };
                    Box::new(OperationRecord {
                        owner: or.owner.clone(),
                        input: or.input.clone(),
                        state: OperationState::ExecuteCas(cas_list),
                    })
                },
                OperationState::ExecuteCas(cas_list) => {
                    let outcome = self.cas_executor(cas_list, &mut ContentionMeasure(0));
                    Box::new(OperationRecord {
                        owner: or.owner.clone(),
                        input: or.input.clone(),
                        state: OperationState::PostCas(cas_list.clone(), outcome),
                    })
                },
                OperationState::PostCas(cas_list, outcome) => {
                     
                     match self.algorithm
                            .wrap_up(*outcome, cas_list, &mut ContentionMeasure(0))
                    {
                        Ok(Some(result)) =>{
                        Box::new(OperationRecord {
                            owner: or.owner.clone(),
                            input: or.input.clone(),
                            state: OperationState::Completed(result),
                        })
                    },
                    Ok(None) => {
                        Box::new(OperationRecord {
                            owner: or.owner.clone(),
                            input: or.input.clone(),
                            state: OperationState::PreCas,
                        })
                    },
                    Err(Contention) => {
                        continue;
                    }
                    // } else {
                        // Box::new(OperationRecord {
                        //     owner: or.owner.clone(),
                        //     input: or.input.clone(),
                        //     state: OperationState::PreCas,
                        // })
                    // }
                }
            }
            };
            let updated_or = Box::into_raw(updated_or);
            if orb
                .val
                .compare_exchange_weak(
                    or as *const OperationRecord<_> as *mut OperationRecord<_>,
                    updated_or,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                let _ = unsafe { Box::from_raw(updated_or) };
            }
        }
    }
}

    pub fn help(&self) {
        if let Some(help) = self.help.peek() {
            self.help_op(unsafe { &*help });
        }
    }

    pub fn run(&self, op: LF::Input) -> LF::Output {
        // if false {
        //     self.help();
        // }

        let mut fast = true;
        for retry in 0.. {
            let help = true/*once in a while false*/ ;
            if retry == 0 {
                if help {
                    self.help();
                }
            } else {
            }

            fast = false;
            let mut contention = ContentionMeasure(0);

            //  if contention.use_slow_path() {}
            let cases = self.algorithm.generate(&op, &mut contention);
            if contention.use_slow_path() {
                break;
            }

            let result = self.cas_executor(&cases, &mut contention);
            if contention.use_slow_path() {
                break;
            }

            match self.algorithm.wrap_up(result, &cases, &mut contention) {
                Ok(outcome) => return outcome,
                Err(()) => {}
            }
            if contention.use_slow_path() {
                break;
            }

            if retry > RETRY_THRESHOLD {
                break;
            }

            // match self.cas_executor(&cases, &mut contention) {
            // Ok(()) => {
            //     self.algorithm.wrap_up(&cases);
            // }

            // if let Err(i) = result {
            //     let help = Help {
            //         completed: AtomicBool::new(false),
            //         at: AtomicUsize::new(i),
            //     };
            //     self.help.add(&help);
            //     while !help.completed.load(std::sync::atomic::Ordering::SeqCst) {
            //         self.help();
            //     }
            // }

            // todo!()
        }
        unreachable!();
        //slow path
        //  if let Err(i) = result {
        let i = 0;
        let orb = OperationRecordBox {
            val: AtomicPtr::new(Box::into_raw(Box::new(OperationRecord {
                owner: std::thread::current().id(),
                input: op,
                state: OperationState::PreCas,
            }))),
            // completed: AtomicBool::new(false),
            // at: AtomicUsize::new(i),
        };
        self.help.enqueue(&orb);
        loop {
            let or = unsafe { &*orb.val.load(Ordering::SeqCst) };
            if let OperationState::Completed(t) = &or.state {
                break t.clone();
            } else {
                self.help();
            }
        }
    
        // while !unsafe { &*orb.val.load(Ordering::SeqCst) }
        //     .state
        //     .is_completed()
        // {
        //     self.help();
        // }
        //  todo!();
    }
}

// todo!()

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
