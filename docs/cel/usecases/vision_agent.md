---
title: CEL Vision Agent
description: Using the Native SDK to pass raw binary image data into a CEL pipeline via parameters.
---

# CEL Usecase: Vision Agent

When dealing with computer vision, you often need to pass massive binary blobs (like images) into the execution pipeline. Converting an image to Base64 to pass it as a string is a massive anti-pattern that bloats memory by 33%. 

By using the **CEL Native SDK**, you can pass the raw byte array natively into the pipeline using the `?` parameterized query syntax.

## The Pipeline

This pipeline:
1. Takes a raw image byte array from the host language.
2. Passes it to an OCR plugin.
3. If text is found, it sends it to an LLM to categorize the document.

```cel
let $image = ?1
let $ocr_text = use plugin::vision -> invoke(ocr, image: $image)

if ($ocr_text != "") {
    use plugin::llm -> invoke(categorize, text: $ocr_text)
} else {
    "NO_TEXT_FOUND"
}
```

## Executing from Node.js SDK

Because we are using the `node-ffi-napi` SDK bindings, the Node.js `Buffer` is passed to Rust as a `*const u8` (C-ABI pointer). The image is never converted to Base64, and the memory is shared across the boundary in 0ms.

```javascript
const fs = require('fs');
const cluaiz = require('cluaiz-sdk');

const cel_script = `
let $image = ?1
let $ocr_text = use plugin::vision -> invoke(ocr, image: $image)

if ($ocr_text != "") {
    use plugin::llm -> invoke(categorize, text: $ocr_text)
} else {
    "NO_TEXT_FOUND"
}
`;

// 1. Read the image as a raw Node.js Buffer
const imageBuffer = fs.readFileSync('./invoice.png');

// 2. Pass it as Parameter ?1 natively
// The SDK handles binding the Buffer to `ExtensionPayload::RawBytes`
const category = cluaiz.execute(cel_script, [imageBuffer]);

console.log(`Document Category: ${category}`);
```

## Architectural Data Flow

```mermaid
flowchart TD
    A["Node.js: execute(script, [Buffer])"] --> B{"CEL Engine (Rust)"}
    
    B -->|?1 (RawBytes)| C["Plugin: vision (OCR)"]
    
    C -->|String Pointer| D{"if ($ocr_text != '')"}
    
    D -->|True| E["Plugin: llm"]
    D -->|False| F["Return 'NO_TEXT_FOUND'"]
    
    E -->|Categorized String| G["ExtensionPayload"]
    F -->|Raw String| G
    
    G -->|FFI Boundary| H["Node.js String"]
```
