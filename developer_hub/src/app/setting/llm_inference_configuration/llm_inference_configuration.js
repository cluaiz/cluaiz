import { Dropdown } from '../../../../components/dropdown/dropdown.js?v=3';

export async function mount(container) {
    let ggufConfig = {
        hardware_and_execution: {},
        templating_flags: {},
        samplers: {},
        user_moved_flags: {}
    };
    let onnxConfig = {
        user_moved_flags: {}
    };

    // Load initial configs from standard endpoint (if available) or fallback to empty structures
    try {
        const ggufRes = await fetch(window.getApiBaseUrl() + '/v1/system/gguf_config').catch(() => null);
        if (ggufRes && ggufRes.ok) {
            ggufConfig = await ggufRes.json();
        }

        const onnxRes = await fetch(window.getApiBaseUrl() + '/v1/system/onnx_config').catch(() => null);
        if (onnxRes && onnxRes.ok) {
            onnxConfig = await onnxRes.json();
        }
    } catch (e) {
        console.error("Failed to load configs", e);
    }

    const saveGguf = async () => {
        try {
            await fetch(window.getApiBaseUrl() + '/v1/system/gguf_config', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(ggufConfig)
            });
        } catch (e) { console.error("Auto-save GGUF failed", e); }
    };

    const saveOnnx = async () => {
        try {
            await fetch(window.getApiBaseUrl() + '/v1/system/onnx_config', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(onnxConfig)
            });
        } catch (e) { console.error("Auto-save ONNX failed", e); }
    };

    // ----- UI Helpers -----

    const setupCustomDropdown = (containerId, descId, mapping, optionsArr, configObj, section, key, onSave) => {
        const dropContainer = container.querySelector('#' + containerId);
        if (dropContainer) {
            let configVal;
            if (section) {
                if (!configObj[section]) configObj[section] = {};
                configVal = configObj[section][key];
            } else {
                configVal = configObj[key];
            }

            let initialValue = configVal !== undefined ? String(configVal) : optionsArr[0].value;

            const dropdown = new Dropdown({
                options: optionsArr,
                defaultValue: initialValue,
                onChange: async (val) => {
                    let finalVal = val;
                    if (val === 'true') finalVal = true;
                    if (val === 'false') finalVal = false;

                    if (section) configObj[section][key] = finalVal;
                    else configObj[key] = finalVal;

                    document.dispatchEvent(new CustomEvent('ggufConfigChanged', { detail: { key, value: finalVal } }));
                    await onSave();
                }
            });
            dropContainer.appendChild(dropdown.render());
        }
    };

    const setupToggle = (containerId, configObj, section, key, onSave) => {
        const toggle = container.querySelector('#' + containerId);
        if (toggle) {
            let configVal;
            if (section) {
                if (!configObj[section]) configObj[section] = {};
                configVal = configObj[section][key];
            } else {
                configVal = configObj[key];
            }

            if (configVal) {
                toggle.classList.add('active');
            } else {
                toggle.classList.remove('active');
            }

            toggle.addEventListener('click', async () => {
                const isActive = !toggle.classList.contains('active');
                if (isActive) toggle.classList.add('active');
                else toggle.classList.remove('active');

                if (section) configObj[section][key] = isActive;
                else configObj[key] = isActive;

                await onSave();
            });
        }
    };

    const setupInput = (inputId, configObj, section, key, isNumber, onSave) => {
        const input = container.querySelector('#' + inputId);
        if (input) {
            let configVal;
            if (section) {
                if (!configObj[section]) configObj[section] = {};
                configVal = configObj[section][key];
            } else {
                configVal = configObj[key];
            }

            if (configVal !== undefined) {
                input.value = configVal;
            }

            input.addEventListener('change', async () => {
                let val = input.value;
                if (isNumber) val = parseFloat(val);

                if (section) configObj[section][key] = val;
                else configObj[key] = val;

                await onSave();
            });
        }
    };

    const setupJsonTextarea = (inputId, configObj, section, key, onSave) => {
        const textarea = container.querySelector('#' + inputId);
        if (textarea) {
            let configVal;
            if (section) {
                if (!configObj[section]) configObj[section] = {};
                configVal = configObj[section][key];
            } else {
                configVal = configObj[key];
            }

            if (configVal !== undefined) {
                textarea.value = JSON.stringify(configVal, null, 2);
            }

            textarea.addEventListener('change', async () => {
                try {
                    const parsed = JSON.parse(textarea.value);
                    if (section) configObj[section][key] = parsed;
                    else configObj[key] = parsed;
                    await onSave();
                } catch (e) {
                    // Invalid JSON, ignore or show visual error
                    textarea.style.borderColor = 'red';
                    setTimeout(() => textarea.style.borderColor = 'var(--border-color)', 2000);
                }
            });
        }
    };

    const setupSamplers = (containerId, configObj, section, key, onSave) => {
        const dropContainer = container.querySelector('#' + containerId);
        if (!dropContainer) return;

        let targetObj = section ? (configObj[section] || (configObj[section] = {})) : configObj;
        if (!targetObj[key]) targetObj[key] = {};
        let samplersObj = targetObj[key];

        const samplerKeys = ['temperature', 'top_k', 'top_p', 'min_p', 'presence_penalty', 'repeat_penalty'];
        dropContainer.innerHTML = '';

        samplerKeys.forEach(sKey => {
            const wrapper = document.createElement('div');
            wrapper.style.display = 'flex';
            wrapper.style.flexDirection = 'column';
            wrapper.style.gap = '4px';

            const label = document.createElement('label');
            label.textContent = sKey.replace('_', ' ').toUpperCase();
            label.style.fontSize = '0.8rem';
            label.style.color = 'var(--text-secondary)';

            const input = document.createElement('input');
            input.type = 'number';
            input.step = sKey === 'top_k' ? '1' : '0.01';
            input.className = 'setting-input';
            input.style.cssText = 'background-color: var(--bg-panel, rgba(0,0,0,0.2)); border: 1px solid var(--border, rgba(255,255,255,0.1)); color: var(--text-main, #fff); padding: 8px 12px; border-radius: 6px; outline: none; font-size: 0.9rem;';
            input.value = samplersObj[sKey] !== undefined ? samplersObj[sKey] : '';

            input.onchange = async (e) => {
                const val = parseFloat(e.target.value);
                if (!isNaN(val)) {
                    samplersObj[sKey] = val;
                } else {
                    delete samplersObj[sKey];
                }
                await onSave();
            };

            wrapper.appendChild(label);
            wrapper.appendChild(input);
            dropContainer.appendChild(wrapper);
        });
    };

    const setupKeyValueMap = (containerId, toggleId, configObj, section, key, onSave) => {
        const dropContainer = container.querySelector('#' + containerId);
        const toggleSwitch = container.querySelector('#' + toggleId);
        if (!dropContainer || !toggleSwitch) return;

        let targetObj = section ? (configObj[section] || (configObj[section] = {})) : configObj;
        if (!targetObj[key]) targetObj[key] = {};
        let mapObj = targetObj[key];

        // Silent cleanup for legacy architecture keys
        let needsCleanupSave = false;
        ['_mode_enabled'].forEach(k => {
            if (mapObj[k] !== undefined) {
                delete mapObj[k];
                needsCleanupSave = true;
            }
        });
        Object.keys(targetObj[key]).forEach(k => {
            if (k.startsWith('_meta_')) {
                delete targetObj[key][k];
            }
        });
        if (!targetObj[key]) targetObj[key] = {};
        mapObj = targetObj[key];

        let isModeEnabled = mapObj.type !== 'custom';

        const renderMap = () => {
            dropContainer.innerHTML = '';
            
            if (isModeEnabled) {
                toggleSwitch.classList.add('active');
                mapObj.type = 'predefined';
                
                if (!mapObj.think_on) mapObj.think_on = { 
                    "Think_Deep": { "0.0": "System Constraint Prompt" }, 
                    "Think_Lite": { "0.5": "[SYSTEM CONSTRAINT: Provide a balanced and informative response.]" } 
                };
                if (!mapObj.think_off) mapObj.think_off = { 
                    "Long_Answer": { "0.8": "[SYSTEM CONSTRAINT: Provide a detailed and comprehensive response.]" }, 
                    "Short_Answer": { "1.0": "[SYSTEM CONSTRAINT: Provide a concise and direct response.]" } 
                };

                // Clean up any stray keys that shouldn't be here
                Object.keys(mapObj).forEach(k => {
                    if (k !== 'type' && k !== 'think_on' && k !== 'think_off') delete mapObj[k];
                });

                const currentThinkMode = (configObj.user_moved_flags && configObj.user_moved_flags.think_mode) || 'Auto';

                let modes = [];
                if (currentThinkMode === 'On' || currentThinkMode === 'Auto') {
                    if(mapObj.think_on['Think_Deep']) modes.push({ id: '0.0', title: 'Think Deep', map: mapObj.think_on['Think_Deep'] });
                    if(mapObj.think_on['Think_Lite']) modes.push({ id: '0.5', title: 'Think Lite', map: mapObj.think_on['Think_Lite'] });
                } 
                if (currentThinkMode === 'Off' || currentThinkMode === 'Auto') {
                    if(mapObj.think_off['Long_Answer']) modes.push({ id: '0.8', title: 'Long Answer', map: mapObj.think_off['Long_Answer'] });
                    if(mapObj.think_off['Short_Answer']) modes.push({ id: '1.0', title: 'Short Answer', map: mapObj.think_off['Short_Answer'] });
                }

                modes.forEach(mode => {
                    const row = document.createElement('div');
                    row.style.cssText = 'display: flex; gap: 10px; align-items: center; margin-bottom: 12px;';

                    const titleSpan = document.createElement('span');
                    titleSpan.textContent = mode.title;
                    titleSpan.style.cssText = 'width: 120px; font-size: 0.85rem; font-weight: 600; color: var(--accent-color, #3b82f6);';

                    const actualTemp = Object.keys(mode.map)[0] || mode.id;
                    const actualPrompt = mode.map[actualTemp] || '';

                    const tempInput = document.createElement('input');
                    tempInput.type = 'text';
                    tempInput.value = actualTemp;
                    tempInput.readOnly = true; 
                    tempInput.style.cssText = 'width: 80px; background-color: var(--bg-panel, rgba(0,0,0,0.1)); border: 1px solid var(--border, rgba(255,255,255,0.05)); color: var(--text-secondary, #aaa); padding: 8px 12px; border-radius: 6px; outline: none; font-weight: 600; font-size: 0.9rem; text-align: center; cursor: not-allowed;';

                    const promptInput = document.createElement('input');
                    promptInput.type = 'text';
                    promptInput.placeholder = 'System Constraint Prompt';
                    promptInput.value = actualPrompt;
                    promptInput.style.cssText = 'flex: 1; background-color: var(--bg-panel, rgba(0,0,0,0.2)); border: 1px solid var(--border, rgba(255,255,255,0.1)); color: var(--text-main, #fff); padding: 8px 12px; border-radius: 6px; outline: none; font-size: 0.9rem;';

                    promptInput.onchange = async () => {
                        mode.map[actualTemp] = promptInput.value;
                        await onSave();
                    };

                    row.appendChild(titleSpan);
                    row.appendChild(tempInput);
                    row.appendChild(promptInput);
                    dropContainer.appendChild(row);
                });
            } else {
                toggleSwitch.classList.remove('active');
                mapObj.type = 'custom';
                delete mapObj.think_on;
                delete mapObj.think_off;
                
                // Get the first custom key (ignoring 'type')
                let customKey = '';
                let customVal = '';
                const keys = Object.keys(mapObj).filter(k => k !== 'type');
                if (keys.length > 0) {
                    customKey = keys[0];
                    customVal = mapObj[customKey];
                    // Clean up extra keys
                    for (let i = 1; i < keys.length; i++) delete mapObj[keys[i]];
                } else {
                    customKey = '0.8';
                    customVal = '[SYSTEM CONSTRAINT: Provide a detailed and comprehensive response.]';
                    mapObj[customKey] = customVal;
                    onSave(); // Ensure it gets saved if it was empty
                }

                const row = document.createElement('div');
                row.style.cssText = 'display: flex; gap: 10px; align-items: center; margin-bottom: 12px;';

                const tempInput = document.createElement('input');
                tempInput.type = 'text';
                tempInput.placeholder = 'Temp (e.g. 0.5)';
                tempInput.value = customKey;
                tempInput.style.cssText = 'width: 80px; background-color: var(--bg-panel, rgba(0,0,0,0.2)); border: 1px solid var(--border, rgba(255,255,255,0.1)); color: var(--text-main, #fff); padding: 8px 12px; border-radius: 6px; outline: none; font-weight: 600; font-size: 0.9rem; text-align: center;';

                const promptInput = document.createElement('input');
                promptInput.type = 'text';
                promptInput.placeholder = 'System Constraint Prompt';
                promptInput.value = customVal;
                promptInput.style.cssText = 'flex: 1; background-color: var(--bg-panel, rgba(0,0,0,0.2)); border: 1px solid var(--border, rgba(255,255,255,0.1)); color: var(--text-main, #fff); padding: 8px 12px; border-radius: 6px; outline: none; font-size: 0.9rem;';

                const updateEntry = async () => {
                    const newTemp = tempInput.value.trim();
                    const newPrompt = promptInput.value;
                    
                    Object.keys(mapObj).forEach(k => { if (k !== 'type') delete mapObj[k]; });
                    if (newTemp !== '') mapObj[newTemp] = newPrompt;
                    await onSave();
                };

                tempInput.onchange = updateEntry;
                promptInput.onchange = updateEntry;

                row.appendChild(tempInput);
                row.appendChild(promptInput);
                dropContainer.appendChild(row);
            }
        };

        toggleSwitch.addEventListener('click', async () => {
            isModeEnabled = !isModeEnabled;
            if (isModeEnabled) {
                mapObj.type = 'predefined';
                Object.keys(mapObj).forEach(k => { if (k !== 'type' && k !== 'think_on' && k !== 'think_off') delete mapObj[k]; });
            } else {
                mapObj.type = 'custom';
                delete mapObj.think_on;
                delete mapObj.think_off;
            }
            renderMap();
            await onSave();
        });

        renderMap();
        if (needsCleanupSave) {
            onSave();
        }

        document.addEventListener('ggufConfigChanged', (e) => {
            if (e.detail.key === 'think_mode') {
                renderMap();
            }
        });
        document.addEventListener('onnxConfigChanged', (e) => {
            if (e.detail.key === 'think_mode') {
                renderMap();
            }
        });
    };

    const makeOptions = (values, labels) => values.map((v, i) => ({ value: String(v), label: labels ? labels[i] : String(v) }));

    // ----- GGUF INITIALIZATION -----

    // Hardware & Execution
    const customDropdownOptions = [
        { value: '-1', label: 'Full GPU (Auto)' },
        { value: '0', label: 'CPU Only' },
        { isInput: true, placeholder: 'Hybrid Layers (e.g. 16)', suffix: 'Layers', inputType: 'number' }
    ];

    setupCustomDropdown('container-gguf-gpu-layers', undefined, undefined,
        customDropdownOptions,
        ggufConfig, 'hardware_and_execution', 'n_gpu_layers', saveGguf);

    setupToggle('toggle-gguf-no-mmap', ggufConfig, 'hardware_and_execution', 'no_mmap', saveGguf);
    setupInput('gguf-override-tensor', ggufConfig, 'hardware_and_execution', 'override_tensor', false, saveGguf);

    setupCustomDropdown('container-gguf-batch-size', undefined, undefined,
        makeOptions(['128', '256', '512', '1024', '2048', '4096', '8192']),
        ggufConfig, 'hardware_and_execution', 'batch_size', saveGguf);

    setupCustomDropdown('container-gguf-ubatch-size', undefined, undefined,
        makeOptions(['128', '256', '512', '1024', '2048']),
        ggufConfig, 'hardware_and_execution', 'ubatch_size', saveGguf);

    setupCustomDropdown('container-gguf-cache-k', undefined, undefined,
        makeOptions(['q5_0', 'q8_0', 'f16']),
        ggufConfig, 'hardware_and_execution', 'cache_type_k', saveGguf);

    setupCustomDropdown('container-gguf-cache-v', undefined, undefined,
        makeOptions(['q4_1', 'q8_0', 'f16']),
        ggufConfig, 'hardware_and_execution', 'cache_type_v', saveGguf);

    setupCustomDropdown('container-gguf-parallel', undefined, undefined,
        makeOptions(['1', '2', '4', '8']),
        ggufConfig, 'hardware_and_execution', 'parallel', saveGguf);

    setupCustomDropdown('container-gguf-spec-type', undefined, undefined,
        makeOptions(['', 'draft-mtp', 'ngram-mod'], ['None', 'draft-mtp', 'ngram-mod']),
        ggufConfig, 'hardware_and_execution', 'spec_type', saveGguf);

    setupCustomDropdown('container-gguf-spec-max', undefined, undefined,
        makeOptions(['0', '1', '2', '3', '5']),
        ggufConfig, 'hardware_and_execution', 'spec_draft_n_max', saveGguf);

    // Templating
    setupInput('gguf-chat-template', ggufConfig, 'templating_flags', 'chat_template_file', false, saveGguf);
    setupInput('gguf-chat-kwargs', ggufConfig, 'templating_flags', 'chat_template_kwargs', false, saveGguf);
    setupToggle('toggle-gguf-jinja', ggufConfig, 'templating_flags', 'jinja', saveGguf);
    setupCustomDropdown('container-gguf-fit', undefined, undefined,
        makeOptions(['off', 'on']),
        ggufConfig, 'templating_flags', 'fit', saveGguf);

    // Samplers
    setupSamplers('container-gguf-samplers', ggufConfig, null, 'samplers', saveGguf);

    // User Moved Flags
    setupCustomDropdown('container-gguf-think-mode', undefined, undefined,
        makeOptions(['Auto', 'On', 'Off']),
        ggufConfig, 'user_moved_flags', 'think_mode', saveGguf);
    setupKeyValueMap('container-gguf-response-length', 'toggle-gguf-mode-enabled', ggufConfig, 'user_moved_flags', 'response_length', saveGguf);


    // ----- ONNX INITIALIZATION -----
    
    // Check system control for Apple Silicon
    let isAppleSilicon = false;
    try {
        const sysRes = await fetch(window.getApiBaseUrl() + '/v1/system/control').catch(() => null);
        if (sysRes && sysRes.ok) {
            const sysData = await sysRes.json();
            if (sysData?.identity?.os_target === 'macOS' || sysData?.silicon_truth?.memory?.is_unified_memory) {
                isAppleSilicon = true;
            }
        }
    } catch(e) {}

    let onnxHardwareOptions = isAppleSilicon ? 
        makeOptions(['Auto'], ['Auto (Apple Silicon)']) : 
        makeOptions(['Auto', 'CPU', 'GPU']);

    setupCustomDropdown('container-onnx-hardware-offload', undefined, undefined,
        onnxHardwareOptions,
        onnxConfig, null, 'hardware_offload', saveOnnx);

    setupCustomDropdown('container-onnx-intra-threads', undefined, undefined,
        makeOptions(['0', '1', '2', '4', '8', '16'], ['Auto (0)', '1', '2', '4', '8', '16']),
        onnxConfig, null, 'intra_op_num_threads', saveOnnx);

    setupCustomDropdown('container-onnx-graph-opt', undefined, undefined,
        makeOptions(['ORT_ENABLE_ALL', 'ORT_ENABLE_BASIC', 'ORT_DISABLE_ALL']),
        onnxConfig, null, 'graph_optimization_level', saveOnnx);

    setupToggle('toggle-onnx-profiling', onnxConfig, null, 'enable_profiling', saveOnnx);

    setupCustomDropdown('container-onnx-inter-threads', undefined, undefined,
        makeOptions(['0', '1', '2', '4', '8', '16'], ['Auto (0)', '1', '2', '4', '8', '16']),
        onnxConfig, null, 'inter_op_num_threads', saveOnnx);

    setupToggle('toggle-onnx-mem-pattern', onnxConfig, null, 'enable_mem_pattern', saveOnnx);
    setupToggle('toggle-onnx-cpu-arena', onnxConfig, null, 'enable_cpu_mem_arena', saveOnnx);

    setupCustomDropdown('container-onnx-exec-mode', undefined, undefined,
        makeOptions(['ORT_SEQUENTIAL', 'ORT_PARALLEL']),
        onnxConfig, null, 'execution_mode', saveOnnx);

    setupCustomDropdown('container-onnx-gpu-limit', undefined, undefined,
        makeOptions(['0', '2147483648', '4294967296', '8589934592'], ['Unlimited (0)', '2 GB', '4 GB', '8 GB']),
        onnxConfig, null, 'gpu_mem_limit_bytes', saveOnnx);

    setupCustomDropdown('container-onnx-arena-strategy', undefined, undefined,
        makeOptions(['kNextPowerOfTwo', 'kSameAsRequested']),
        onnxConfig, null, 'arena_extend_strategy', saveOnnx);

    setupToggle('toggle-onnx-ort-opt', onnxConfig, null, 'enable_ort_transformers_optimization', saveOnnx);

    setupCustomDropdown('container-onnx-kv-data-type', undefined, undefined,
        makeOptions(['ort_fp16', 'ort_fp32']),
        onnxConfig, null, 'kv_cache_data_type', saveOnnx);

    setupToggle('toggle-onnx-deterministic', onnxConfig, null, 'use_deterministic_compute', saveOnnx);

    setupCustomDropdown('container-onnx-think-mode', undefined, undefined,
        makeOptions(['Auto', 'On', 'Off']),
        onnxConfig, 'user_moved_flags', 'think_mode', saveOnnx);

    setupKeyValueMap('container-onnx-response-length', 'toggle-onnx-mode-enabled', onnxConfig, 'user_moved_flags', 'response_length', saveOnnx);


    // Tab switching logic (using tools_setting design)
    const tabs = container.querySelectorAll('.inference-tab-btn');
    const contents = container.querySelectorAll('.inference-tab-content');

    tabs.forEach(tab => {
        tab.addEventListener('click', () => {
            // Remove active styling from all tabs
            tabs.forEach(t => {
                t.classList.remove('active');
                t.style.background = 'transparent';
                t.style.color = 'var(--text-secondary)';
            });
            contents.forEach(c => c.style.display = 'none');

            // Add active styling to clicked tab
            tab.classList.add('active');
            tab.style.background = 'var(--accent-color, #3b82f6)';
            tab.style.color = '#fff';

            // Show target content
            const targetId = tab.getAttribute('data-target');
            const targetContent = container.querySelector('#' + targetId);
            if (targetContent) {
                targetContent.style.display = 'block';
            }
        });
    });
}