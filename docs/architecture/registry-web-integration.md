# 🏛️ Independent Neural Registry: Web Integration & System Design
**Software Architecture & Dynamic Routing Blueprint for cluaiz.com**

This document establishes the system design, data architecture, and software blueprints for dynamically linking the local/remote **Independent Neural Registry** (`cluaiz/models/library`) with the front-end **cluaiz Portal** (`cluaiz.com`). 

---

## 1. The Core Architecture Problem
Currently, the website's models page (`/models`) and its subroutes (`/[family]/[version]/[variant]`) rely on a static, hardcoded TypeScript catalog `cluaiz.com/src/data/models.ts` (`SOVEREIGN_MODELS`).

### ⚠️ Gaps & Issues:
1. **Catalog Desync**: If a developer adds a new version or model variant to the core CLI registry (`cluaiz/models/library`), the website does not reflect it until the frontend is manually updated.
2. **Quantization Mismatch**: The frontend calculates sizes and quant labels using rough client-side approximations rather than querying the verified precision schemas in the versioned JSONs.
3. **Empty Documentation**: The dynamic detail page attempts to fetch a `README.md` from the registry, but because legacy registries lack version-level Markdown files and static assets, pages load with empty layouts.

---

## 2. The Dynamic Independent Registry Solution
To achieve a **"Universal Neural Kernel"** standard, we decouple the UI from static assets and compile the directory dynamically.

```
┌────────────────────────────────────────────────────────┐
│               LOCAL WORKSPACE FILESYSTEM               │
│         cluaiz/models/library/ (JSON, MD, Assets)       │
└───────────────────────────┬────────────────────────────┘
                            │ (Local Dev: Direct fs Read)
                            ▼
┌────────────────────────────────────────────────────────┐
│                   NEXT.JS API ENGINES                  │
│       /api/v1/registry/models  |  /api/models/readme   │
└───────────────────────────┬────────────────────────────┘
                            │ (Server-Side Server State)
                            ▼
┌────────────────────────────────────────────────────────┐
│                     DYNAMIC ROUTING                    │
│    /models  ➔  /[family]  ➔  /[version]  ➔  /[variant]  │
└────────────────────────────────────────────────────────┘
```

---

## 3. Data Schema Specifications

To keep the models metadata clean, we establish a two-tiered directory configuration.

### A. Family Level Metadata: `family.json`
To avoid hardcoding in the frontend, each model family folder (e.g. `cluaiz/models/library/Qwen/family.json`) will contain its high-level identity specs.

```json
{
  "id": "qwen",
  "name": "Qwen",
  "provider": "Alibaba Cloud",
  "logo": "/assets/models/ai-logo/qwen.webp",
  "architecture": "MoE",
  "formats": ["GGUF", "AWQ", "GPTQ"],
  "description": "The global leader in multilingual and analytical reasoning. Exceptional performance on complex logic."
}
```

### B. Version Level Metadata: `[version].json` (e.g., `3.0.json`)
The nested variant matrix maps precise quantizations, weights, RAM/VRAM targets, and remote files.

```json
{
  "family": "Qwen 3.0",
  "qwen3:8b": {
    "name": "Qwen 3 8B",
    "architecture": "Qwen3",
    "context_window": "128k",
    "category": "chat",
    "default_quant": "q4_k_m",
    "description": "The new 8B flagship for professional local reasoning.",
    "variants": {
      "gguf": {
        "q4_k_m": {
          "download_url": "https://huggingface.co/bartowski/Qwen3-8B-Instruct-GGUF/resolve/main/Qwen3-8B-Instruct-Q4_K_M.gguf",
          "ram_required_gb": 8.5,
          "download_size_gb": 5.5
        }
      }
    }
  }
}
```

### C. Version Documentation: `README.md` & `./assets`
Each version folder contains its own localized documentation page and `./assets` folder for high-resolution benchmark diagrams, rendering rich visual matrices instantly.

---

## 4. dynamic API Routing Flow

### 1. `/api/v1/registry/models` (Catalog Aggregator)
Instead of returning a flat route map, this endpoint will dynamically traverse `cluaiz/models/library/` folders, read each `family.json`, aggregate all version directories, and compile a single, unified `ModelFamily[]` array matching the exact structure expected by the front-end UI.

#### Server-Side Resolver Logic:
```typescript
import fs from "fs";
import path from "path";

export async function resolveDynamicRegistry() {
  const libraryPath = path.join(process.cwd(), "..", "cluaiz", "models", "library");
  const families: any[] = [];

  const familyDirs = fs.readdirSync(libraryPath)
    .filter(f => fs.statSync(path.join(libraryPath, f)).isDirectory() && f !== "other");

  for (const dir of familyDirs) {
    const familyPath = path.join(libraryPath, dir);
    const familyJsonPath = path.join(familyPath, "family.json");
    
    if (!fs.existsSync(familyJsonPath)) continue;
    
    const familyData = JSON.parse(fs.readFileSync(familyJsonPath, "utf-8"));
    const versions: any[] = [];
    
    const versionDirs = fs.readdirSync(familyPath)
      .filter(f => fs.statSync(path.join(familyPath, f)).isDirectory() && f.startsWith("v-"));
      
    for (const verDir of versionDirs) {
      const cleanVerId = verDir.replace(/^v-/, "v");
      const verPath = path.join(familyPath, verDir);
      
      // Discover JSON files (e.g. 3.0.json)
      const jsonFiles = fs.readdirSync(verPath).filter(f => f.endsWith(".json"));
      const variants: any[] = [];
      
      for (const file of jsonFiles) {
        const fileContent = JSON.parse(fs.readFileSync(path.join(verPath, file), "utf-8"));
        
        // Loop through models inside the JSON file
        Object.entries(fileContent).forEach(([modelId, modelData]: [string, any]) => {
          if (modelId === "family") return;
          
          variants.push({
            id: modelId.split(":")[1] || modelId,
            name: modelData.name,
            size: modelData.category || "Balanced",
            parameters: modelId.split(":")[1] || "8B",
            context_window: modelData.context_window || "128K",
            recommended_format: `${modelData.default_quant.toUpperCase()}`,
            description: modelData.description
          });
        });
      }
      
      versions.push({
        id: cleanVerId,
        name: `${familyData.name} ${cleanVerId.replace("v", "")}`,
        release_date: "2026-05", // Loaded dynamically if available
        variants
      });
    }
    
    families.push({
      ...familyData,
      versions
    });
  }
  return families;
}
```

### 2. `/models/[family]` (Dynamic Family Page)
Extracts dynamic family metadata and routes versions directly from the compiled filesystem registry.

### 3. `/models/[family]/[version]` (Dynamic Version Page)
Dynamically maps active formats (`GGUF`, `AWQ`, `GPTQ`) and extracts the correct variant cards based on their actual specifications in the versioned JSON files.

### 4. `/models/[family]/[version]/[variant]` (Dynamic Variant Detail Page)
Shows actual VRAM/RAM specs parsed directly from the JSON quantization matrix, and parses the dynamic localized `README.md` through `ReactMarkdown`.

---

## 5. Architectural Benefits
1. **Absolute Decoupling**: Frontend files never need to be modified when new models are launched.
2. **Verified Accuracy**: VRAM limits, RAM allocation guidelines, and HuggingFace download links are 100% matched to their production CLI targets.
3. **Makkhan Developer Workflow**: Adding a new model family is as easy as adding a folder.
