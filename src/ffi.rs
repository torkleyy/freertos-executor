#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::c_void;

pub type BaseType_t = i32;
pub type UBaseType_t = u32;
pub type TickType_t = u32;
pub type NotifyAction = i32;

pub type QueueHandle_t = *mut c_void;
pub type TaskHandle_t = *mut c_void;

pub const NOTIFY_ACTION_INCREMENT: NotifyAction = 2;

unsafe extern "C" {
    pub fn xQueueGenericCreate(
        queue_length: UBaseType_t,
        item_size: UBaseType_t,
        queue_type: u8,
    ) -> QueueHandle_t;

    pub fn xQueueGenericSend(
        queue: QueueHandle_t,
        item: *const c_void,
        ticks_to_wait: TickType_t,
        copy_position: BaseType_t,
    ) -> BaseType_t;

    pub fn xQueueGenericSendFromISR(
        queue: QueueHandle_t,
        item: *const c_void,
        higher_priority_task_woken: *mut BaseType_t,
        copy_position: BaseType_t,
    ) -> BaseType_t;

    pub fn xQueueReceive(
        queue: QueueHandle_t,
        buffer: *mut c_void,
        ticks_to_wait: TickType_t,
    ) -> BaseType_t;

    pub fn uxQueueMessagesWaiting(queue: QueueHandle_t) -> UBaseType_t;

    pub fn vQueueDelete(queue: QueueHandle_t);

    pub fn xTaskGetCurrentTaskHandle() -> TaskHandle_t;

    pub fn xTaskGenericNotify(
        task_to_notify: TaskHandle_t,
        notify_index: UBaseType_t,
        value: u32,
        action: NotifyAction,
        previous_notification_value: *mut u32,
    ) -> BaseType_t;

    pub fn xTaskGenericNotifyFromISR(
        task_to_notify: TaskHandle_t,
        notify_index: UBaseType_t,
        value: u32,
        action: NotifyAction,
        previous_notification_value: *mut u32,
        higher_priority_task_woken: *mut BaseType_t,
    ) -> BaseType_t;

    pub fn xPortInIsrContext() -> BaseType_t;

    pub fn vPortYieldFromISR();
}
