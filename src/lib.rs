use std::{
    ffi::c_void,
    future::{poll_fn, Future},
    marker::PhantomData,
    mem::size_of,
    rc::Rc,
    sync::{
        atomic::{AtomicPtr, Ordering},
        Arc,
    },
    task::Poll,
};

use async_task::{Runnable, Task};

mod ffi;

const QUEUE_TYPE_BASE: u8 = 0;
const SEND_TO_BACK: ffi::BaseType_t = 0;
const NO_WAIT: ffi::TickType_t = 0;
const NOTIFY_INDEX_0: ffi::UBaseType_t = 0;
const DEFAULT_QUEUE_CAPACITY: usize = 2048;
const RUN_QUEUE_BATCH: usize = 200;

type RunnablePtr = *mut Runnable;

/// Lightweight single-task executor backed by a FreeRTOS queue.
///
/// This executor uses `async_task::spawn_local`, so tasks must be spawned and
/// polled from the same FreeRTOS task.
///
/// It is assumed that the executor is dropped when its creation task is destroyed.
/// Tasks that were not driven to completion will be leaked and can never complete;
/// it is assumed that either you ensure all tasks complete before dropping the executor
/// or the device will be reset because of an error condition.
pub struct LocalExecutor {
    // spawn_local still requires the scheduler to be Send+Sync
    state: Arc<State>,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl LocalExecutor {
    pub fn new() -> Self {
        Self {
            state: Arc::new(State::new()),
            _not_send_sync: PhantomData,
        }
    }

    pub fn spawn<F>(&self, future: F) -> Task<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let state = Arc::clone(&self.state);
        let schedule = move |runnable| state.schedule(runnable);

        let (runnable, task) = async_task::spawn_local(future, schedule);
        runnable.schedule();

        task
    }

    /// Must only be called once.
    ///
    /// This assumes that the runner executes on a FreeRTOS task.
    /// We use low level task notification mechanisms that are ISR-safe,
    /// which cannot be implemented in a portable way.
    pub async fn run<T>(&self, future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);

        poll_fn(|cx| {
            let has_more = self.state.run_queued(RUN_QUEUE_BATCH);

            match future.as_mut().poll(cx) {
                Poll::Ready(output) => Poll::Ready(output),
                Poll::Pending => {
                    if has_more {
                        cx.waker().wake_by_ref();
                    }
                    Poll::Pending
                }
            }
        })
        .await
    }
}

struct State {
    queue: ffi::QueueHandle_t,
    runner: AtomicPtr<c_void>,
}

// SAFETY: `State` is shared between tasks only through thread-safe primitives:
// - FreeRTOS queue/task-notify APIs are designed for cross-task use.
// - `runner` and counters are atomic.
unsafe impl Send for State {}
// SAFETY: same reasoning as `Send`; all shared mutation is synchronized.
unsafe impl Sync for State {}

impl State {
    fn new() -> Self {
        assert!(
            DEFAULT_QUEUE_CAPACITY <= ffi::UBaseType_t::MAX as usize,
            "executor queue capacity exceeds FreeRTOS UBaseType_t range"
        );

        let queue = unsafe {
            ffi::xQueueGenericCreate(
                DEFAULT_QUEUE_CAPACITY as ffi::UBaseType_t,
                size_of::<RunnablePtr>() as ffi::UBaseType_t,
                QUEUE_TYPE_BASE,
            )
        };

        assert!(!queue.is_null(), "failed to allocate FreeRTOS queue");
        let runner = unsafe { ffi::xTaskGetCurrentTaskHandle() };
        assert!(!runner.is_null(), "failed to get FreeRTOS task handle");

        Self {
            queue,
            runner: AtomicPtr::new(runner.cast()),
        }
    }

    fn schedule(&self, runnable: Runnable) {
        // this will be leaked if the executor is dropped
        let runnable_ptr: RunnablePtr = Box::into_raw(Box::new(runnable));
        let in_isr = unsafe { ffi::xPortInIsrContext() != 0 };
        let mut higher_prio_task_woken: ffi::BaseType_t = 0;

        let item = (&runnable_ptr as *const RunnablePtr).cast();
        let sent = unsafe {
            if in_isr {
                ffi::xQueueGenericSendFromISR(
                    self.queue,
                    item,
                    &mut higher_prio_task_woken,
                    SEND_TO_BACK,
                )
            } else {
                ffi::xQueueGenericSend(self.queue, item, NO_WAIT, SEND_TO_BACK)
            }
        };

        if sent == 0 {
            // we never expect the queue to fill up completely, panic as last resort
            unsafe {
                drop(Box::from_raw(runnable_ptr));
            }
            panic!("executor wake queue overflow");
        }

        self.notify_runner(in_isr, &mut higher_prio_task_woken);

        if in_isr && higher_prio_task_woken != 0 {
            unsafe {
                ffi::vPortYieldFromISR();
            }
        }
    }

    fn run_queued(&self, max_tasks: usize) -> bool {
        let mut ran = 0;

        while ran < max_tasks {
            if self.run_one_freertos() {
                ran += 1;
                continue;
            }

            return false;
        }

        self.has_queued_work()
    }

    fn run_one_freertos(&self) -> bool {
        let mut ptr: RunnablePtr = std::ptr::null_mut();

        let received =
            unsafe { ffi::xQueueReceive(self.queue, (&mut ptr as *mut RunnablePtr).cast(), NO_WAIT) };

        if received == 0 {
            return false;
        }

        self.run_runnable_ptr(ptr);
        true
    }

    fn has_queued_work(&self) -> bool {
        unsafe { ffi::uxQueueMessagesWaiting(self.queue) > 0 }
    }

    fn run_runnable_ptr(&self, ptr: RunnablePtr) {
        if ptr.is_null() {
            panic!("executor received null runnable pointer");
        }

        let runnable = unsafe { *Box::from_raw(ptr) };
        runnable.run();
    }

    fn notify_runner(&self, in_isr: bool, higher_prio_task_woken: &mut ffi::BaseType_t) {
        let runner: ffi::TaskHandle_t = self.runner.load(Ordering::Acquire).cast();

        if runner.is_null() {
            return;
        }

        unsafe {
            if in_isr {
                ffi::xTaskGenericNotifyFromISR(
                    runner,
                    NOTIFY_INDEX_0,
                    0,
                    ffi::NOTIFY_ACTION_INCREMENT,
                    std::ptr::null_mut(),
                    higher_prio_task_woken,
                );
            } else {
                ffi::xTaskGenericNotify(
                    runner,
                    NOTIFY_INDEX_0,
                    0,
                    ffi::NOTIFY_ACTION_INCREMENT,
                    std::ptr::null_mut(),
                );
            }
        }
    }
}

impl Drop for LocalExecutor {
    fn drop(&mut self) {
        // if the executor is used on the stack, this ensures the task can no longer be notified
        self.state
            .runner
            .store(std::ptr::null_mut(), Ordering::Release);
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.runner.store(std::ptr::null_mut(), Ordering::Release);

        while self.run_one_freertos() {}

        // SAFETY: we can safely drop the queue as all references to state
        // are gone
        unsafe {
            ffi::vQueueDelete(self.queue);
        }
    }
}

