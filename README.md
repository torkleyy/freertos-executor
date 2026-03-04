# freertos-executor

A very simple async executor using FreeRTOS queues and task notification
for task scheduling and task parking / unparking.

Many choices have been made to keep the executor simple and efficient,
but limiting the flexibility and safety. Namely, the executor
does not release memory of ongoing tasks when dropped.

This executor is meant to be run together with a `block_on`
using FreeRTOS task notifications (e.g. `esp_idf_hal::task::block_on`).

An even better approach for when the executor should run infinitely anyways would
be to get rid of the `run` method and the main future, and block the task
on the queue itself.

I've tested this with an ESP IDF v5.5 project.

## Disclaimer

FreeRTOS is a registered trademark of Amazon Web Services, Inc.
ESP-IDF and ESP32 are trademarks or registered trademarks of
Espressif Systems (Shanghai) Co., Ltd.
This project is independent and is not affiliated with or endorsed by
Amazon Web Services, Inc. or Espressif Systems (Shanghai) Co., Ltd.
