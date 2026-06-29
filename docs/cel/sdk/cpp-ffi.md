---
title: CEL C++ SDK
description: Embedding cluaiz plugins in C++ using C-ABI bindings.
---

# CEL C++ SDK

C++ plugins are incredibly fast and can easily manage memory dynamically using smart pointers. However, to interface with the cluaiz Engine, you must expose an `extern "C"` boundary that speaks the `ExtensionPayload` struct protocol.

## The Memory Struct

We define the C struct inside an `extern "C"` block.

```cpp
#include <cstdint>
#include <cstdlib>
#include <string>
#include <cstring>

extern "C" {
    typedef enum {
        Json = 0,
        Cdql = 1,
        WasmBinary = 2,
        RawBytes = 3,
        Bincode = 4
    } PayloadType;

    typedef struct {
        PayloadType payload_type;
        const uint8_t* data_ptr;
        size_t data_len;
    } ExtensionPayload;
}
```

## Creating a C++ SDK Plugin

You can use modern C++ features inside your function, but the boundaries must be raw C pointers.

### 1. The Execution Function

```cpp
#if defined(_WIN32)
#define EXPORT __declspec(dllexport)
#else
#define EXPORT __attribute__((visibility("default")))
#endif

extern "C" EXPORT ExtensionPayload* process_data(const ExtensionPayload* input) {
    if (!input) return nullptr;
    
    // 1. Read input as std::string (Assuming it's JSON)
    std::string in_str(reinterpret_cast<const char*>(input->data_ptr), input->data_len);
    
    // 2. Perform modern C++ logic
    std::string out_str = "{\"status\": \"processed_by_cpp\", \"original\": " + in_str + "}";
    
    // 3. Allocate outgoing C memory
    // We must use raw new or malloc because the pointer crosses the ABI boundary.
    uint8_t* buffer = new uint8_t[out_str.length()];
    std::memcpy(buffer, out_str.data(), out_str.length());
    
    ExtensionPayload* out_payload = new ExtensionPayload();
    out_payload->payload_type = Json;
    out_payload->data_ptr = buffer;
    out_payload->data_len = out_str.length();
    
    return out_payload;
}
```

## Memory Management

Because you used `new`, you are responsible for `delete`. The Engine will call `cluaiz_free_payload`.

### 2. The Free Function

```cpp
extern "C" EXPORT void cluaiz_free_payload(ExtensionPayload* ptr) {
    if (!ptr) return;
    
    if (ptr->data_ptr) {
        delete[] ptr->data_ptr;
    }
    
    delete ptr;
}
```

## Architectural Flow

```mermaid
flowchart TD
    A["CEL: invoke(cpp_plugin)"] --> B{"cluaiz Engine"}
    
    B -->|Allocate| C["ExtensionPayload Pointer"]
    
    C -->|dlopen/dlsym| D["extern 'C' boundary"]
    
    D -->|std::string copy| E["Modern C++ Object"]
    E -->|Logic| F["New std::string"]
    
    F -->|new uint8_t[]| G["Raw C++ heap allocation"]
    G -->|new ExtensionPayload| H["New ExtensionPayload Pointer"]
    
    H -->|Return Pointer| B
    B -->|Engine Reads Data| I["Pipeline Continues"]
    
    I -->|Engine Calls FFI| J["cluaiz_free_payload(Ptr)"]
    J --> K["delete[] removes memory from OS"]
```
