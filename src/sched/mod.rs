use collections::Vec;
use collections::boxed::Box;
use core::cell::RefCell;
use core::cell::Cell;
use super::platform;
use alloc::boxed::FnBox;
use super::thread;

use platform::ThreadId;
use sync;


const WAKE_NEVER: u64 = 0xFFFFFFFF_FFFFFFFF;

// TODO: make this Thread and SMP safe.
// TODO this is the one mega unsafe class, so it needs to take care of it's on safety.
pub struct Sched {
    sched_impl : RefCell<SchedImpl>,
    cpu_mutex : sync::CpuMutex<()>
}

struct SchedImpl {
    threads: Vec<Box<thread::Thread>>,
    idle_thread: thread::Thread,
    
    curr_thread_index: Option<usize>,
    thread_id_counter: usize,
    time_since_boot_millies: u64,
}

const IDLE_THREAD_ID: ThreadId = ThreadId(0);
const MAIN_THREAD_ID: ThreadId = ThreadId(1);

extern "C" fn thread_start(start: *mut Box<FnBox()>) {
    unsafe {
        let f: Box<Box<FnBox()>> = Box::from_raw(start);
        f();
        platform::get_platform_services().get_scheduler().exit_thread();
    }
}

impl Sched {
    pub fn new() -> Sched {
        Sched {
        cpu_mutex : sync::CpuMutex::new(()), 
        sched_impl : RefCell::new(SchedImpl {
            // fake thread as this main thread..
            threads: vec![Box::new(
                thread::Thread::new_cur_thread(MAIN_THREAD_ID)
                )
                ],
            idle_thread: thread::Thread::new(IDLE_THREAD_ID, ::mem::VirtualAddress(platform::wait_for_interrupts as usize), 0),
            curr_thread_index : Some(0),
            thread_id_counter : 10,
            time_since_boot_millies : 0,
        })
        }
    }

    pub fn spawn<F>(&self, f: F)
        where F: FnOnce(),
              F: Send + 'static
    {
        let p: Box<FnBox()> = Box::new(f);
        let ptr = Box::into_raw(Box::new(p)) as *const usize as usize; // some reson without another box ptr is 1
        self.spawn_thread(::mem::VirtualAddress(thread_start as usize), ptr);
    }

    pub fn spawn_thread(&self,
                        start: ::mem::VirtualAddress,
                        arg: usize) {
        // TODO thread safety and SMP Support
        let mut simpl = self.sched_impl.borrow_mut();
        simpl.thread_id_counter +=1;

        let t = Box::new(thread::Thread::new(ThreadId(simpl.thread_id_counter), start, arg));

        let ig = platform::intr::no_interrupts();
        let guard = self.cpu_mutex.lock();
        simpl.threads.push(t);
        // find an eligble thread
        // threads.map()
    }

    fn schedule_new(&self) -> &thread::Thread {
        // find an eligble thread
        // threads.map()
        let mut simpl = self.sched_impl.borrow_mut();

        let mut cur_th = if let Some(cur_th_i) = simpl.curr_thread_index { cur_th_i} else { 0 };

        let num_threads = simpl.threads.len();
        for _ in 0..num_threads {
            cur_th += 1;
            // TODO linker with libgcc/compiler_rt so we can have division and mod
            if cur_th == num_threads {
                cur_th = 0;
            }

            {
                let time_since_boot_millies = simpl.time_since_boot_millies;
                let cur_thread = &mut simpl.threads[cur_th];
                if (cur_thread.wake_on != WAKE_NEVER) &&
                   (cur_thread.wake_on <= time_since_boot_millies) {
                    cur_thread.wake_on = 0;
                    cur_thread.ready = true;
                }
            }

            let cur_thread_ready = simpl.threads[cur_th].ready;
            if cur_thread_ready {
                simpl.curr_thread_index = Some(cur_th);
                let thread_ref = simpl.threads[cur_th].as_ref();
                return thread_ref;
            }
        }
        // no thread is ready.. time to sleep sleep...
        // return to the idle thread.
        // don't wait for interrupts here, as we might already be in an interrupt..
        simpl.curr_thread_index = None;
        &simpl.idle_thread
    }

    pub fn exit_thread(&self) {
        // disable interrupts
        let ig = platform::intr::no_interrupts();
        let guard = self.cpu_mutex.lock();

        {
            let mut simpl = self.sched_impl.borrow_mut();
            // remove the current thread
            let cur_th = simpl.curr_thread_index.unwrap();
            simpl.threads.remove(cur_th);
        }
        let new_context = self.schedule_new();
        // tmp ctx.. we don't really gonna use it...

        // TODO: is this smp safe? probably yes..
        platform::switch_context(None, &new_context);

        // TODO - stack leaks here.. 
        // need to register the thread for clean up..

    }

    pub fn yield_thread(&self) {
        // disable interrupts
        let ig = platform::intr::no_interrupts();
        let guard = self.cpu_mutex.lock();
        self.yeild_thread_no_intr()

    }

    fn yeild_thread_no_intr(&self) {

        let curr_thread = { self.sched_impl.borrow().curr_thread_index };

        // TODO: should we add a mutex for smp?

        let new_context = self.schedule_new();

        let new_thread = { self.sched_impl.borrow().curr_thread_index };


        if curr_thread != new_thread {
            let t = self.sched_impl.borrow_mut().threads[curr_thread.unwrap()].as_ref();
                        
            let oldT = platform::switch_context(Some(t), &new_context);
            // we get here when context is switch back to us
            // TODO: SMP: mark that the old thread is no longer running on CPU

        }
    }

    pub fn unschedule_no_intr(&self) {
        let mut simpl = self.sched_impl.borrow_mut();
        let cur_th = simpl.curr_thread_index.unwrap();

        let ref mut t = simpl.threads[cur_th];
        t.ready = false;
        if t.wake_on == 0 {
            t.wake_on = WAKE_NEVER;
        }
    }

    pub fn block(&self) {
        // disable interrupts
        let ig = platform::intr::no_interrupts();
        let guard = self.cpu_mutex.lock();
        self.block_no_intr()
    }

    pub fn sleep(&self, millis: u32) {
        // disable interrupts
        //TODO how to release cpu guard after the context was saved?!
        let ig = platform::intr::no_interrupts();
        {
            let guard = self.cpu_mutex.lock();

            let mut simpl = self.sched_impl.borrow_mut();
            let time_since_boot_millies = simpl.time_since_boot_millies;
            let cur_th = simpl.curr_thread_index.unwrap();
            let ref mut cur_thread = simpl.threads[cur_th];
            cur_thread.wake_on = time_since_boot_millies + (millis as u64);
            cur_thread.ready = false;
        }
        self.yeild_thread_no_intr()
    }

    // assume interrupts are blocked
    pub fn block_no_intr(&self) {
        {
            let mut simpl = self.sched_impl.borrow_mut();
            let cur_th = simpl.curr_thread_index.unwrap();

            let ref mut t = simpl.threads[cur_th];
            t.ready = false;
            if t.wake_on == 0 {
                t.wake_on = WAKE_NEVER;
            }
        }
        self.yeild_thread_no_intr();
    }

    pub fn wakeup(&self, tid: ThreadId) {
        // disable interrupts
        let ig = platform::intr::no_interrupts();
        self.wakeup_no_intr(tid)
    }

    // assume interrupts are blocked
    pub fn wakeup_no_intr(&self, tid: ThreadId) {
        let mut simpl = self.sched_impl.borrow_mut();

        for t in simpl.threads.iter_mut().filter(|x| x.id == tid) {
            // there can only be one..
            t.wake_on = 0;
            t.ready = true;
            break;
        }
    }

    pub fn get_current_thread(&self) -> ThreadId {
        let simpl = self.sched_impl.borrow();

        return simpl.threads[simpl.curr_thread_index.unwrap()].id;
    }

    // TODO
    pub fn lock(&mut self) {}

    pub fn unlock(&mut self) {}

}

// this function runs in the context of whatever thread was interrupted.
fn handle_interrupts() {
    let ig = platform::intr::no_interrupts();
    
    // copy context to stack;
  //  let ctx = *get_cur_cpu().get_context();
    // switch!
   // yield();


}


// for the timer interrupt..
impl platform::InterruptSource for Sched {
    // this method is called platform::ticks_in_second times a second
    fn interrupted(&self, ctx: &mut platform::Context) {
        const DELTA_MILLIS: u64= (1000 / platform::ticks_in_second) as u64;
        {
            let mut simpl = self.sched_impl.borrow_mut();
            simpl.time_since_boot_millies +=  DELTA_MILLIS;
        }
        // TODO: change to yeild? - we need yield to mark the unscheduled 
        // thread as unscheduled.. so it can continue to run on other cpus.. 
        // and release cpu mutex..
        // TODO(YES!): switch to CPU SCHEDULER THREAD (per cpu) not in interrupt mode..
        self.yeild_thread_no_intr();
        // TODO: need to notify that context was switched
        // set pc to handle_interrupts
        // set r0 to lr
    }
}
