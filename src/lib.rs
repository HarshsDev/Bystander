use std::{clone, result};
// use std::intrinsics::simd::simd_bitmask;
// use std::clone;
use std::marker::PhantomData;
use std::ops::Index;
// use std::process::Output;
use std::sync::atomic::Ordering::{self};
use std::sync::atomic::{AtomicPtr, AtomicU8};

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

// pub trait Cas {
//    // type Meta;
//     fn execute(&self) -> Result<(), ()>;
// }

#[repr(u8)]
#[derive(PartialEq, Clone, Debug)]
enum CasState {
    Success,
    Failure,
    Pending,
}

struct CasByRcu<T> {
    //cond for doing wap
    version: u64,
    // meta: M,

    //the value to cas
    value: T,
}

// struct TripleMarked<T> {
//     value:T,
//     mark: bool,
//     flag: bool,
//     help: bool,
// }

// pub struct VersionedTripleMarkRefernce<T> (Atomic<TripleMarked<T>>);

// impl<T> VersionedTripleMarkRefernce<T> {
//     pub fn new(value: T, mark:bool, flag: bool, help:bool) -> Self {
//         Self(Atomic::new(TripleMarked { value, mark, flag, help }))
//     }

//     pub fn get_reference(&self) -> &T {
//         self.0.
//     }
// }

pub struct Atomic<T>(AtomicPtr<CasByRcu<T>>);

pub trait VersionedCas {
    fn execute(&self, contention: &mut ContentionMeasure) -> Result<bool, Contention>;
    fn has_modified_bit(&self) -> bool;
    fn clear_bit(&self) -> bool;
    fn state(&self) -> CasState;
    fn set_state(&self, new: CasState);
}

// struct TripleMarked<T> {
//     value:T,
//     mark: bool,
//     flag: bool,
//     help: bool,
// }

// pub struct VersionedTripleMarkRefernce<T> (Atomic<TripleMarked<T>>);

// impl<T> VersionedTripleMarkRefernce<T> {
//     pub fn new(value: T, mark:bool, flag: bool, help:bool) -> Self {
//         Self(Atomic::new(TripleMarked { value, mark, flag, help }))
//     }

//     pub fn get_reference(&self) -> &T {
//         self.0.with(|(v,_)| &v.value)
//     }

//     pub fn is_marked(&self) -> bool {
//         self.0.with(|(v,_)| v.mark)
//     }

//      pub fn is_help(&self) -> bool {
//         self.0.with(|(v,_)| v.help)
//     }

//      pub fn is_flag(&self) -> bool {
//         self.0.with(|(v,_)| v.flag)
//     }
// }

impl<T> Atomic<T>
where
    T: PartialEq + Eq + Copy,
{
    pub fn new(initial: T) -> Self {
        Self(AtomicPtr::new(Box::into_raw(Box::new(CasByRcu {
            version: 0,
            value: initial,
        }))))
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T, u64) -> R,
    {
        let this_ptr = self.get();
        let this = unsafe { &*this_ptr };
        f(&this.value, this.version)
    }

    // fn is_help_in_version(&self, version: usize) {
    //     self.with(|(_, mfh, v)| v == version && mfh.help)
    // }

    fn get(&self) -> *mut CasByRcu<T> {
        self.0.load(Ordering::SeqCst)
    }

    pub fn value(&self) -> &T {
        &unsafe { &*self.get() }.value
    }

    // pub fn meta(&self) -> &M {
    //     &unsafe { &*self.0.load(Ordering::SeqCst) }.meta
    // }

    pub fn set(&self, new: T) {
        let this_ptr = self.0.load(Ordering::SeqCst);
        let this = unsafe { &*this_ptr };
        if this.value != new {
            self.0.store(
                Box::into_raw(Box::new(CasByRcu {
                    version: this.version + 1,
                    // meta: new_meta,
                    value: new,
                })),
                Ordering::SeqCst,
            );
        }
    }

    pub fn compare_and_set(&self, expected: &T, value: T) -> bool {
        let this_ptr = self.0.load(Ordering::SeqCst);
        let this = unsafe { &*this_ptr };
        if this.value == *expected {
            if *expected != value {
                self.0.compare_exchange(
                    this_ptr,
                    Box::into_raw(Box::new(CasByRcu {
                        version: this.version + 1,
                        // meta: new_meta,
                        value,
                    })),
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                );
            }
            true
        } else {
            false
        }
    }

    pub fn compare_and_set_versioned(
        &self,
        expected: &T,
        value: T,
        contention: &mut ContentionMeasure,
        version: Option<u64>,
    ) -> Result<bool, Contention> {
        let this_ptr = self.0.load(Ordering::SeqCst);
        let this = unsafe { &*this_ptr };
        if &this.value == expected {
            if let Some(v) = version {
                if v != this.version {
                    contention.detected()?;
                    return Ok(false);
                }
            }
            if expected == &value {
                return Ok(true);
            } else {
                let new_ptr = Box::into_raw(Box::new(CasByRcu {
                    version: this.version + 1,
                    value,
                }));
                match self.0.compare_exchange(
                    this_ptr,
                    new_ptr,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => Ok(true),
                    Err(current) => {
                        let _ = unsafe {
                            Box::from_raw(new_ptr);
                        };
                        contention.detected()?;
                        Ok(false)
                    }
                }
            }
        } else {
            Ok(false)
        }
    }

    fn execute(&self) -> bool {
        todo!()
    }

    fn has_modified_bit(&self) -> bool {
        todo!()
    }

    fn clear_bit(&self) -> bool {
        todo!()
    }

    fn set_state(&self, new: &CasState) {
        todo!()
    }

    fn state(&self) -> CasState {
        todo!()
    }
}

// pub trait CasDescriptors<D>: Index<usize, Output = D>
// where
//     DCas:
// {
//     fn len(&self) -> usize {
//         todo!()
//     }
// }

pub trait NormalizedLockFree {
    type Input: Clone;
    type Output: Clone;
    // type cas: CasDescriptor;
    type CommitDescriptor: Clone;
    fn generate(
        &self,
        op: &Self::Input,
        contention: &mut ContentionMeasure,
    ) -> Result<Self::CommitDescriptor, Contention>;
    // fn execute(
    //     &self,
    //     cases:Self::Descriptor,
    //     i: usize,
    //     contention: ContentionMeasure,
    // ) -> Result<(), usize>;
    fn wrap_up(
        &self,
        executed: Result<(), usize>,
        performed: &Self::CommitDescriptor,
        contention: &mut ContentionMeasure,
    ) -> Result<Option<Self::Output>, Contention>;

    fn fast_path(
        &self,
        op: &Self::Input,
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
    ExecuteCas(LF::CommitDescriptor),
    PostCas(LF::CommitDescriptor, Result<(), usize>),
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
    LF::CommitDescriptor: Clone,
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
where
    for<'a> &'a LF::CommitDescriptor: IntoIterator<Item = &'a dyn VersionedCas>, // where
                                                                                 //     OperationRecord<LF>: Clone,
{
    fn cas_executor(
        &self,
        descriptors: &LF::CommitDescriptor,
        contention: &mut ContentionMeasure,
    ) -> Result<Result<(), usize>, Contention> {
        // let len = descriptors.len();
        for (i, cas) in descriptors.into_iter().enumerate() {
            match cas.state() {
                CasState::Success => {
                    cas.clear_bit();
                }
                CasState::Failure => {
                    return Ok(Err(i));
                }
                CasState::Pending => {
                    cas.execute(contention)?;
                    if cas.has_modified_bit() {
                        cas.set_state(CasState::Success);
                        cas.clear_bit();
                    }

                    if cas.state() != CasState::Success {
                        cas.set_state(CasState::Failure);
                        return Ok(Err(i));
                    }
                }
            }
        }
        Ok(Ok(()))
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
                }
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
                }
                OperationState::ExecuteCas(cas_list) => {
                    let outcome = match self.cas_executor(cas_list, &mut ContentionMeasure(0)) {
                        Ok(outcome) => outcome,
                        Err(Contention) => continue,
                    };

                    Box::new(OperationRecord {
                        owner: or.owner.clone(),
                        input: or.input.clone(),
                        state: OperationState::PostCas(cas_list.clone(), outcome),
                    })
                }
                OperationState::PostCas(cas_list, outcome) => {
                    match self
                        .algorithm
                        .wrap_up(*outcome, cas_list, &mut ContentionMeasure(0))
                    {
                        Ok(Some(result)) => Box::new(OperationRecord {
                            owner: or.owner.clone(),
                            input: or.input.clone(),
                            state: OperationState::Completed(result),
                        }),
                        Ok(None) => Box::new(OperationRecord {
                            owner: or.owner.clone(),
                            input: or.input.clone(),
                            state: OperationState::PreCas,
                        }),
                        Err(Contention) => {
                            continue;
                        } // } else {
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
            // let help = true/*once in a while false*/ ;
            // if retry == 0 {
            //     if help {
            //         self.help();
            //     }
            // } else {
            // }

            // fast = false;
            let mut contention = ContentionMeasure(0);

            match self.algorithm.fast_path(&op, &mut contention) {
                Ok(Some(result)) => {
                    return result;
                }
                Ok(None) => {}
                Err(Contention) => {}
            }
            //  if contention.use_slow_path() {}
            // let cases = match self.algorithm.generate(&op, &mut contention) {
            //     Ok(c) => c,
            //     Err(Contention) => {
            //         // generation failed due to contention; retry
            //         break;
            //     }
            // };
            // if contention.use_slow_path() {
            //     break;
            // }

            // let result = match self.cas_executor(&cases, &mut contention) {
            //     Ok(result) => result,
            //     Err(Contention) => break,
            // };
            // if contention.use_slow_path() {
            //     break;
            // }

            // match self.algorithm.wrap_up(result, &cases, &mut contention) {
            //     Ok(Some(outcome)) => return outcome,
            //     Ok(None) => {}
            //     Err(Contention) => {
            //         break;
            //     }
            // }
            // // if contention.use_slow_path() {
            //     break;
            // }

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
        // unreachable!();
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
