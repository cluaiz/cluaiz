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

    let headers = { 'Content-Type': 'application/json' };
    
    // Fetch auth token if required
    try {
        const pRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission').catch(() => null);
        if (pRes && pRes.ok) {
            const pData = await pRes.json();
            if (pData.permission && pData.permission.api_auth && pData.permission.api_auth.required && pData.permission.api_auth.tokens.length > 0) {
                headers['Authorization'] = 'Bearer ' + pData.permission.api_auth.tokens[0];
            }
        }
    } catch (e) {
        console.error("Failed to fetch permission", e);
    }

    // Load initial configs from standard endpoint (if available) or fallback to empty structures
    try {
        const ggufRes = await fetch(window.getApiBaseUrl() + '/v1/system/gguf_config', { headers }).catch(() => null);
        if (ggufRes && ggufRes.ok) {
            ggufConfig = await ggufRes.json();
        }

        const onnxRes = await fetch(window.getApiBaseUrl() + '/v1/system/onnx_config', { headers }).catch(() => null);
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
                headers: headers,
                body: JSON.stringify(ggufConfig)
            });
        } catch (e) { console.error("Auto-save GGUF failed", e); }
    };

    const saveOnnx = async () => {
        try {
            await fetch(window.getApiBaseUrl() + '/v1/system/onnx_config', {
                method: 'POST',
                headers: headers,
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

            let initialValue = optionsArr[0]?.value || '';
            if (configVal !== undefined && configVal !== null) {
                if (typeof configVal === 'object') {
                    initialValue = optionsArr[0]?.value || '';
                    if (section) configObj[section][key] = initialValue;
                    else configObj[key] = initialValue;
                } else {
                    const strVal = String(configVal);
                    const matchedOption = optionsArr.find(o => !o.isInput && String(o.value).toLowerCase() === strVal.toLowerCase());
                    if (matchedOption) {
                        initialValue = String(matchedOption.value);
                    } else {
                        initialValue = strVal;
                    }
                }
            }

            const dropdown = new Dropdown({
                options: optionsArr,
                defaultValue: initialValue,
                onChange: async (val) => {
                    let finalVal = val;
                    if (val === 'true') finalVal = true;
                    else if (val === 'false') finalVal = false;
                    else if (typeof val === 'string' && val.trim() !== '' && !isNaN(val)) finalVal = Number(val);

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

        const samplerRanges = {
            temp: { min: 0, max: 2, step: 0.01 },
            top_k: { min: 0, max: 100, step: 1 },
            top_p: { min: 0, max: 1, step: 0.01 },
            min_p: { min: 0, max: 1, step: 0.01 },
            presence_penalty: { min: 0, max: 2, step: 0.01 },
            repeat_penalty: { min: 1, max: 2, step: 0.01 }
        };
        const samplerKeys = Object.keys(samplerRanges);
        dropContainer.innerHTML = '';

        samplerKeys.forEach(sKey => {
            const config = samplerRanges[sKey];
            const wrapper = document.createElement('div');
            wrapper.style.display = 'flex';
            wrapper.style.flexDirection = 'column';
            wrapper.style.gap = '8px';
            wrapper.style.backgroundColor = 'rgba(0,0,0,0.1)';
            wrapper.style.padding = '12px';
            wrapper.style.borderRadius = '8px';

            const headerRow = document.createElement('div');
            headerRow.style.display = 'flex';
            headerRow.style.justifyContent = 'space-between';
            headerRow.style.alignItems = 'center';

            const label = document.createElement('label');
            label.textContent = sKey === 'temp' ? 'TEMPERATURE' : sKey.replace('_', ' ').toUpperCase();
            label.style.fontSize = '0.8rem';
            label.style.color = 'var(--text-secondary)';
            label.style.fontWeight = 'bold';

            const numInput = document.createElement('input');
            numInput.type = 'number';
            numInput.min = config.min;
            numInput.max = config.max;
            numInput.step = config.step;
            numInput.className = 'setting-input';
            numInput.style.cssText = 'background-color: var(--bg-panel, rgba(0,0,0,0.3)); border: 1px solid var(--border, rgba(255,255,255,0.1)); color: var(--text-main, #fff); padding: 4px 8px; border-radius: 4px; outline: none; font-size: 0.8rem; width: 60px; text-align: center;';
            numInput.value = samplersObj[sKey] !== undefined ? samplersObj[sKey] : '';

            headerRow.appendChild(label);
            headerRow.appendChild(numInput);

            const slider = document.createElement('input');
            slider.type = 'range';
            slider.min = config.min;
            slider.max = config.max;
            slider.step = config.step;
            slider.value = samplersObj[sKey] !== undefined ? samplersObj[sKey] : (config.min + (config.max - config.min) / 2);
            slider.style.width = '100%';
            slider.style.cursor = 'pointer';

            const syncValues = async (valStr) => {
                let val = parseFloat(valStr);
                if (!isNaN(val)) {
                    if (val < config.min) val = config.min;
                    if (val > config.max) val = config.max;
                    samplersObj[sKey] = val;
                    numInput.value = val;
                    slider.value = val;
                } else {
                    delete samplersObj[sKey];
                    numInput.value = '';
                }
                await onSave();
            };

            numInput.onchange = (e) => syncValues(e.target.value);
            slider.onchange = (e) => syncValues(e.target.value);
            slider.oninput = (e) => { numInput.value = e.target.value; }; // instant UI update for slider sliding

            wrapper.appendChild(headerRow);
            wrapper.appendChild(slider);
            dropContainer.appendChild(wrapper);
        });
    };

    const setupKeyValueMap = (containerId, toggleId, configObj, section, key, onSave) => {
        const dropContainer = container.querySelector('#' + containerId);
        const toggleSwitch = container.querySelector('#' + toggleId);
        if (!dropContainer || !toggleSwitch) return;

        let targetObj = section ? (configObj[section] || (configObj[section] = {})) : configObj;
        if (!targetObj[key]) targetObj[key] = {};

        // If the backend sent a stringified JSON (like "{}"), parse it to an object first!
        if (typeof targetObj[key] === 'string') {
            try {
                targetObj[key] = JSON.parse(targetObj[key]);
            } catch (e) {
                targetObj[key] = {};
            }
        }
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

            // Update top-level description dynamically (Only explain the feature)
            const topDesc = toggleSwitch.closest('.setting-item').querySelector('.setting-desc');
            if (topDesc) {
                if (isModeEnabled) {
                    topDesc.textContent = 'Predefined Mode: Enables standard built-in modes for the AI engine.';
                } else {
                    topDesc.textContent = 'Custom Mode: Enables fully customizable inputs for every request.';
                }
            }

            // Bottom section: Single, combined, non-colorful description
            const modeDetails = document.createElement('p');
            modeDetails.style.cssText = 'color: var(--text-muted, #9ca3af); font-size: 0.85rem; margin-bottom: 15px; line-height: 1.4;';

            if (isModeEnabled) {
                modeDetails.textContent = 'Temperature controls the AI creativity: 0.0 forces strict logic and factual accuracy, while higher values (up to 2.0) make it more creative and unpredictable. You have 4 predefined options to configure your temperature and system prompts (2 for Thinking Mode ON, 2 for Thinking Mode OFF).';
            } else {
                modeDetails.textContent = 'Temperature controls the AI creativity: 0.0 forces strict logic and factual accuracy, while higher values (up to 2.0) make it more creative and unpredictable. You can manually send your own custom temperature and prompt with every input request, and the AI will adapt dynamically.';
            }
            dropContainer.appendChild(modeDetails);

            if (isModeEnabled) {
                toggleSwitch.classList.add('active');
                mapObj.type = 'predefined';

                if (!mapObj.think_on) mapObj.think_on = {
                    "Think_Deep": { "0.0": "Analyze the request deeply step-by-step. Provide a highly detailed, comprehensive, and deeply reasoned response." },
                    "Think_Lite": { "0.5": "Think carefully but provide a balanced, concise, and to-the-point response." }
                };
                if (!mapObj.think_off) mapObj.think_off = {
                    "Long_Answer": { "0.8": "Provide a detailed, thorough, and to-the-point answer without unnecessary fluff." },
                    "Short_Answer": { "1.0": "Provide a very concise, direct, and to-the-point answer." }
                };

                // Clean up any stray keys that shouldn't be here
                Object.keys(mapObj).forEach(k => {
                    if (k !== 'type' && k !== 'think_on' && k !== 'think_off') delete mapObj[k];
                });

                const currentThinkMode = (configObj.user_moved_flags && configObj.user_moved_flags.think_mode) || 'Auto';

                let modes = [];
                if (currentThinkMode === 'On' || currentThinkMode === 'Auto') {
                    if (mapObj.think_on['Think_Deep']) modes.push({ id: '0.0', title: 'Think Deep', map: mapObj.think_on['Think_Deep'] });
                    if (mapObj.think_on['Think_Lite']) modes.push({ id: '0.5', title: 'Think Lite', map: mapObj.think_on['Think_Lite'] });
                }
                if (currentThinkMode === 'Off' || currentThinkMode === 'Auto') {
                    if (mapObj.think_off['Long_Answer']) modes.push({ id: '0.8', title: 'Long Answer', map: mapObj.think_off['Long_Answer'] });
                    if (mapObj.think_off['Short_Answer']) modes.push({ id: '1.0', title: 'Short Answer', map: mapObj.think_off['Short_Answer'] });
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
                    tempInput.type = 'number';
                    tempInput.min = 0;
                    tempInput.max = 2;
                    tempInput.step = 0.01;
                    tempInput.value = actualTemp;
                    tempInput.style.cssText = 'width: 80px; background-color: var(--bg-panel, rgba(0,0,0,0.3)); border: 1px solid var(--border, rgba(255,255,255,0.2)); color: var(--text-main, #fff); padding: 8px 12px; border-radius: 6px; outline: none; font-weight: 600; font-size: 0.9rem; text-align: center;';

                    tempInput.onchange = async () => {
                        let newTemp = parseFloat(tempInput.value);
                        if (isNaN(newTemp)) newTemp = parseFloat(actualTemp) || 0.0;
                        if (newTemp < 0) newTemp = 0;
                        if (newTemp > 2) newTemp = 2;

                        // Keep max 2 decimal places to avoid crazy long numbers
                        newTemp = Math.round(newTemp * 100) / 100;
                        let newTempStr = newTemp.toString();
                        // ensure it at least has .0 if it's an integer for consistency
                        if (!newTempStr.includes('.')) newTempStr += '.0';

                        tempInput.value = newTempStr;

                        const oldKey = Object.keys(mode.map)[0];
                        const val = mode.map[oldKey];
                        delete mode.map[oldKey];
                        mode.map[newTempStr] = val;

                        await onSave();
                        renderMap();
                    };

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
                }

                const row = document.createElement('div');
                row.style.cssText = 'display: flex; gap: 10px; align-items: center; margin-bottom: 12px;';

                const tempInput = document.createElement('input');
                tempInput.type = 'number';
                tempInput.min = 0;
                tempInput.max = 2;
                tempInput.step = 0.01;
                tempInput.placeholder = 'Temp';
                tempInput.value = customKey;
                tempInput.style.cssText = 'width: 80px; background-color: var(--bg-panel, rgba(0,0,0,0.2)); border: 1px solid var(--border, rgba(255,255,255,0.1)); color: var(--text-main, #fff); padding: 8px 12px; border-radius: 6px; outline: none; font-weight: 600; font-size: 0.9rem; text-align: center;';

                const promptInput = document.createElement('input');
                promptInput.type = 'text';
                promptInput.placeholder = 'System Constraint Prompt';
                promptInput.value = customVal;
                promptInput.style.cssText = 'flex: 1; background-color: var(--bg-panel, rgba(0,0,0,0.2)); border: 1px solid var(--border, rgba(255,255,255,0.1)); color: var(--text-main, #fff); padding: 8px 12px; border-radius: 6px; outline: none; font-size: 0.9rem;';

                const updateEntry = async () => {
                    const tempRaw = tempInput.value.toString().trim();
                    Object.keys(mapObj).forEach(k => { if (k !== 'type') delete mapObj[k]; });

                    if (tempRaw === '') {
                        // Keep it empty
                        await onSave();
                        return;
                    }

                    let newTemp = parseFloat(tempRaw);
                    if (isNaN(newTemp)) newTemp = 0.0;
                    if (newTemp < 0) newTemp = 0;
                    if (newTemp > 2) newTemp = 2;

                    newTemp = Math.round(newTemp * 100) / 100;
                    let newTempStr = newTemp.toString();
                    if (!newTempStr.includes('.')) newTempStr += '.0';

                    tempInput.value = newTempStr;
                    mapObj[newTempStr] = promptInput.value;
                    await onSave();
                    window.dispatchEvent(new CustomEvent('config:response_length_changed'));
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
            window.dispatchEvent(new CustomEvent('config:response_length_changed'));
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

    const nCtxOptions = [
        { value: '0', label: 'Dynamic Auto' },
        { value: '-1', label: 'Max Native' },
        { isInput: true, placeholder: 'Context Size (e.g. 4096)', suffix: 'Tokens', inputType: 'number' }
    ];

    setupCustomDropdown('container-gguf-n-ctx', undefined, undefined,
        nCtxOptions,
        ggufConfig, 'hardware_and_execution', 'n_ctx', saveGguf);

    setupToggle('toggle-gguf-no-mmap', ggufConfig, 'hardware_and_execution', 'no_mmap', saveGguf);

    const overrideTensorOptions = [
        { value: '', label: 'Auto - Full GPU' },
        { value: 'blk\\.(1[0-9]|2[0-9])\\.ffn_.*=CPU', label: 'Offload Middle Layers (8B/7B)' },
        { value: 'blk\\.(2[0-9]|3[0-9]|4[0-3])\\.ffn_.*=CPU', label: 'Offload Middle Layers (70B)' },
        { isInput: true, placeholder: 'Custom Regex (e.g. blk...=CPU)', inputType: 'text' }
    ];
    setupCustomDropdown('container-gguf-override-tensor', undefined, undefined,
        overrideTensorOptions,
        ggufConfig, 'hardware_and_execution', 'override_tensor', saveGguf);

    setupCustomDropdown('container-gguf-batch-size', undefined, undefined,
        makeOptions(['128', '256', '512', '1024', '2048', '4096', '8192']),
        ggufConfig, 'hardware_and_execution', 'batch_size', saveGguf);

    setupCustomDropdown('container-gguf-ubatch-size', undefined, undefined,
        makeOptions(['128', '256', '512', '1024', '2048']),
        ggufConfig, 'hardware_and_execution', 'ubatch_size', saveGguf);

    setupCustomDropdown('container-gguf-parallel', undefined, undefined,
        makeOptions(['1', '2', '4', '8']).concat([{ isInput: true, placeholder: 'Custom', inputType: 'number' }]),
        ggufConfig, 'hardware_and_execution', 'parallel', saveGguf);

    setupCustomDropdown('container-gguf-spec-type', undefined, undefined,
        makeOptions(['', 'draft-mtp', 'ngram-mod'], ['None', 'draft-mtp', 'ngram-mod']),
        ggufConfig, 'hardware_and_execution', 'spec_type', saveGguf);

    setupCustomDropdown('container-gguf-spec-max', undefined, undefined,
        makeOptions(['0', '1', '2', '3', '5']),
        ggufConfig, 'hardware_and_execution', 'spec_draft_n_max', saveGguf);

    // Templating
    setupCustomDropdown('container-gguf-chat-template', undefined, undefined,
        makeOptions([''], ['Auto (Read from Model)']).concat([{ isInput: true, placeholder: 'Custom Template (.jinja)', inputType: 'text' }]),
        ggufConfig, 'templating_flags', 'chat_template_file', saveGguf);

    setupCustomDropdown('container-gguf-chat-kwargs', undefined, undefined,
        makeOptions([''], ['Auto (Default Kwargs)']).concat([{ isInput: true, placeholder: 'Custom JSON (e.g. {"preserve_thinking": true})', inputType: 'text' }]),
        ggufConfig, 'templating_flags', 'chat_template_kwargs', saveGguf);

    setupToggle('toggle-gguf-jinja', ggufConfig, 'templating_flags', 'jinja', saveGguf);
    setupCustomDropdown('container-gguf-fit', undefined, undefined,
        makeOptions(['off', 'on']),
        ggufConfig, 'templating_flags', 'fit', saveGguf);

    // Samplers
    setupSamplers('container-gguf-samplers', ggufConfig, null, 'samplers', saveGguf);

    const thinkModeOptions = [
        { value: 'Auto', label: 'Auto (Model Default)' },
        { value: 'Off', label: 'Off (Think Tag Prefill)' },
        { value: 'Low', label: 'Low (512 Tokens)' },
        { value: 'Medium', label: 'Medium (1024 Tokens)' },
        { value: 'High', label: 'High (Full Reasoning)' },
        { isInput: true, placeholder: 'Custom Tokens (e.g. 768)', suffix: 'Tokens', inputType: 'number' }
    ];

    const responseLengthOptions = [
        { value: 'auto', label: 'Auto (Default)' },
        { value: 'short', label: 'Short (Concise Answers)' },
        { value: 'medium', label: 'Medium (Standard Length)' },
        { value: 'long', label: 'Long (Detailed Answers)' },
        { isInput: true, placeholder: 'Custom Target (e.g. 200)', suffix: 'Tokens', inputType: 'number' }
    ];

    // User Moved Flags
    setupCustomDropdown('container-gguf-think-mode', undefined, undefined,
        thinkModeOptions,
        ggufConfig, 'user_moved_flags', 'think_mode', saveGguf);

    setupCustomDropdown('container-gguf-response-length', undefined, undefined,
        responseLengthOptions,
        ggufConfig, 'user_moved_flags', 'response_length', saveGguf);


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
    } catch (e) { }

    let onnxHardwareOptions = [
        { value: '-1', label: isAppleSilicon ? 'Auto (Apple Silicon)' : 'GPU Full Load (Auto)' },
        { value: '0', label: 'CPU Only' },
        { isInput: true, placeholder: 'Hybrid Layers (e.g. 16)', suffix: 'Layers', inputType: 'number' }
    ];

    setupCustomDropdown('container-onnx-hardware-offload', undefined, undefined,
        onnxHardwareOptions,
        onnxConfig, null, 'n_gpu_layers', saveOnnx);

    setupCustomDropdown('container-onnx-n-ctx', undefined, undefined,
        nCtxOptions,
        onnxConfig, null, 'n_ctx', saveOnnx);

    setupCustomDropdown('container-onnx-intra-threads', undefined, undefined,
        makeOptions(['0', '1', '2', '4', '8', '16'], ['Auto (0)', '1', '2', '4', '8', '16']).concat([{ isInput: true, placeholder: 'Custom', inputType: 'number' }]),
        onnxConfig, null, 'intra_op_num_threads', saveOnnx);

    setupCustomDropdown('container-onnx-graph-opt', undefined, undefined,
        makeOptions(['ORT_ENABLE_ALL', 'ORT_ENABLE_EXTENDED', 'ORT_ENABLE_BASIC', 'ORT_DISABLE_ALL']),
        onnxConfig, null, 'graph_optimization_level', saveOnnx);

    setupToggle('toggle-onnx-profiling', onnxConfig, null, 'enable_profiling', saveOnnx);

    setupCustomDropdown('container-onnx-inter-threads', undefined, undefined,
        makeOptions(['0', '1', '2', '4', '8', '16'], ['Auto (0)', '1', '2', '4', '8', '16']).concat([{ isInput: true, placeholder: 'Custom', inputType: 'number' }]),
        onnxConfig, null, 'inter_op_num_threads', saveOnnx);

    setupToggle('toggle-onnx-mem-pattern', onnxConfig, null, 'enable_mem_pattern', saveOnnx);
    setupToggle('toggle-onnx-cpu-arena', onnxConfig, null, 'enable_cpu_mem_arena', saveOnnx);

    setupCustomDropdown('container-onnx-exec-mode', undefined, undefined,
        makeOptions(['ORT_SEQUENTIAL', 'ORT_PARALLEL']),
        onnxConfig, null, 'execution_mode', saveOnnx);

    setupCustomDropdown('container-onnx-gpu-limit', undefined, undefined,
        makeOptions(['0', '2147483648', '4294967296', '8589934592'], ['Unlimited (0)', '2 GB', '4 GB', '8 GB']).concat([{ isInput: true, placeholder: 'Custom (Bytes)', inputType: 'number' }]),
        onnxConfig, null, 'gpu_mem_limit_bytes', saveOnnx);

    setupCustomDropdown('container-onnx-arena-strategy', undefined, undefined,
        makeOptions(['kNextPowerOfTwo', 'kSameAsRequested']),
        onnxConfig, null, 'arena_extend_strategy', saveOnnx);

    setupToggle('toggle-onnx-ort-opt', onnxConfig, null, 'enable_ort_transformers_optimization', saveOnnx);

    setupCustomDropdown('container-onnx-kv-data-type', undefined, undefined,
        makeOptions(['ort_fp32', 'ort_fp16', 'ort_int8'], ['fp32 (Highest Quality)', 'fp16 (Half memory)', 'int8 (Quarter memory)']),
        onnxConfig, null, 'kv_cache_data_type', saveOnnx);

    setupToggle('toggle-onnx-deterministic', onnxConfig, null, 'use_deterministic_compute', saveOnnx);

    setupCustomDropdown('container-onnx-think-mode', undefined, undefined,
        thinkModeOptions,
        onnxConfig, 'user_moved_flags', 'think_mode', saveOnnx);

    setupCustomDropdown('container-onnx-response-length', undefined, undefined,
        responseLengthOptions,
        onnxConfig, 'user_moved_flags', 'response_length', saveOnnx);


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