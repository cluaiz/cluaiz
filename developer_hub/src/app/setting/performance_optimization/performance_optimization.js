import { Dropdown } from '../../../../components/dropdown/dropdown.js?v=3';
import { showModal } from '../../../../components/modal/modal.js';
import { CodeEditor } from '../../../../components/editor/editor.js';

const DESCRIPTIONS = {
    brainMode: {
        true: 'Enabled: LLM is completely turned off. Only database and tools will run.',
        false: 'Disabled: Normal mode. Generative AI is active.'
    },
    lazyLoad: {
        true: 'Enabled: Model loads only on first message. Saves RAM while idle.',
        false: 'Disabled: Model loads instantly on startup.'
    },

    mlock: {
        'Auto': 'System decides based on RAM availability.',
        'On': 'Forces the OS to lock the model in RAM. Prevents swapping and stuttering.',
        'Off': 'Allows the OS to swap memory if needed.'
    },
    boosterProfile: {
        'balance': 'Balances speed, memory, and CPU usage.',
        'multitasking': 'Leaves room for background apps.',
        'max_boost': 'High performance, uses more resources.',
        'ultra_max_boost': 'Extreme performance, will lag background apps.',
        'hyper_cluster': 'For multi-GPU setups only.',
        'edge': 'Optimized for low-power devices and laptops.'
    },
    flashAttn: {
        'Auto': 'System uses Flash Attention if supported by your hardware.',
        'On': 'Forces Flash Attention. Very fast for long contexts.',
        'Off': 'Disables Flash Attention. Useful if the model hallucinates.'
    },
    context: {
        'Auto': 'System dynamically balances memory consumption and chat history retention.',
        'Off': 'Disables dynamic memory compression. The engine may crash if the conversation becomes excessively long.',
        'Minimal': 'Only retains the current topic and flushes older context (Saves maximum RAM).',
        'Standard': 'Best balance for everyday chats. Retains important older messages while freeing up unused memory.',
        'Aggressive': 'Attempts to compress and retain the entire conversation history. Requires higher CPU processing power.',
        'Extreme': 'Retains absolute context without forgetting. Demands maximum CPU and memory resources.'
    },
    kvQuant: {
        'Auto': 'System decides the best quantization level.',
        'Kv16': 'Highest quality, uses more RAM.',
        'Kv8': 'Good balance of quality and RAM usage.',
        'Kv4': 'Maximum compression. Saves massive RAM but may reduce long-context quality slightly.'
    },
    turbo: {
        'Auto': 'System decides based on available memory bandwidth.',
        'On': 'Compresses tensors dynamically for faster processing.',
        'Off': 'Processes at standard precision.'
    },
    specDec: {
        'Auto': 'System decides whether to use a draft model.',
        'On': 'Generates tokens faster by guessing ahead using a tiny model.',
        'Off': 'Generates token-by-token normally.'
    },
    autoRound: {
        'Auto': 'System decides when to round weights.',
        'On': 'Aggressively compresses the model to save VRAM.',
        'Off': 'Maintains original model weight precision.'
    },
    dflash: {
        'Auto': 'System dynamically allocates flash attention buffers.',
        'On': 'Forces dynamic allocation, saving VRAM at the cost of slight CPU overhead.',
        'Off': 'Pre-allocates buffers. Faster but uses more VRAM.'
    },
    vramReclaim: {
        'Auto': 'System intelligently decides when to free memory based on your current hardware load.',
        'On': 'Instantly flushes GPU memory the moment a reply is finished. Keeps your PC completely smooth.',
        'Off': 'Keeps the AI loaded in memory for instant subsequent replies. Background apps may experience slight lag.'
    },
    gpuLayers: {
        '-1': 'System automatically balances workload. Runs as much on the GPU as possible for optimal speed.',
        '0': 'Restricts the AI to use only the CPU and System RAM. Slower generation, but highly stable and safe.',
        '32': 'Splits the workload evenly between the GPU and CPU. Recommended for systems with limited VRAM.'
    },
    thinkMode: {
        'Auto': 'Engages reasoning processes exclusively for complex mathematical or coding queries.',
        'On': 'Forces the AI to narrate its internal step-by-step reasoning before providing the final answer.',
        'Off': 'Delivers direct answers immediately without displaying its internal thought process.'
    },
    moe: {
        'Auto': 'System decides MoE expert routing.',
        'On': 'Optimizes VRAM strictly for Mixture-of-Experts models.',
        'Off': 'Standard routing.'
    },
    outputStyle: {
        'separated': 'The reasoning process is parsed  and cleanly separated from the final answer.',
        'raw': 'The raw thinking stream including <think> tags is provided directly.'
    }
};

export async function mount(container) {
    let boosterConfig = {};
    let permData = {};
    let installedModels = [];

    // Fetch initial state
    try {
        const [boosterRes, permRes, modelsRes] = await Promise.all([
            fetch(window.getApiBaseUrl() + '/v1/booster/status').catch(() => null),
            fetch(window.getApiBaseUrl() + '/v1/system/permission').catch(() => null),
            fetch(window.getApiBaseUrl() + '/v1/models/installed').catch(() => null)
        ]);

        if (boosterRes && boosterRes.ok) {
            const data = await boosterRes.json();
            boosterConfig = data.booster || {};
        }
        if (permRes && permRes.ok) {
            const data = await permRes.json();
            permData = data.permission || {};
        }
        if (modelsRes && modelsRes.ok) {
            const data = await modelsRes.json();
            installedModels = data.installed || [];
        }
    } catch (e) {
        console.error("Failed to load initial settings:", e);
    }

    const updateBoosterSetting = async (key, value) => {
        try {
            if (key.includes('.')) {
                const parts = key.split('.');
                if (!boosterConfig[parts[0]]) boosterConfig[parts[0]] = {};
                boosterConfig[parts[0]][parts[1]] = value;
            } else {
                boosterConfig[key] = value;
            }
            await fetch(window.getApiBaseUrl() + '/v1/booster/update', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(boosterConfig)
            });
        } catch (e) {
            console.error("Failed to update booster setting:", e);
        }
    };

    const updatePermissionSetting = async (key, value) => {
        try {
            permData[key] = value;
            const payload = { ...permData };
            delete payload.available_models;
            delete payload.available_chat_models;
            delete payload.available_vector_models;
            delete payload.available_vision_models;
            delete payload.available_audio_models;
            delete payload.lan_ip;
            delete payload.status;

            await fetch(window.getApiBaseUrl() + '/v1/system/permission', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(payload)
            });
        } catch (e) {
            console.error("Failed to update permission setting:", e);
        }
    };

    // Helper for toggles
    const setupToggle = (id, descId, mapping, key, isSystemState = false) => {
        const toggle = container.querySelector('#' + id);
        const desc = container.querySelector('#' + descId);
        if (toggle && desc) {
            let isActive = false;
            if (isSystemState) {
                isActive = permData[key] === true;
            } else if (boosterConfig[key] !== undefined) {
                isActive = boosterConfig[key] === 'On' || boosterConfig[key] === true;
            }

            if (isActive) toggle.classList.add('active');
            else toggle.classList.remove('active');

            desc.textContent = mapping[isActive];

            toggle.addEventListener('click', async () => {
                toggle.classList.toggle('active');
                const newState = toggle.classList.contains('active');
                desc.textContent = mapping[newState];

                if (isSystemState) {
                    await updatePermissionSetting(key, newState);
                } else {
                    await updateBoosterSetting(key, newState ? 'On' : 'Off');
                }
            });
        }
    };

    // Helper for custom dropdowns
    const setupCustomDropdown = (containerId, descId, mapping, optionsArr, key, defaultValue, onValueChange) => {
        const dropContainer = container.querySelector('#' + containerId);
        const desc = container.querySelector('#' + descId);
        if (dropContainer && desc) {
            let configVal;
            if (key.includes('.')) {
                const parts = key.split('.');
                configVal = boosterConfig[parts[0]] ? boosterConfig[parts[0]][parts[1]] : undefined;
            } else {
                configVal = boosterConfig[key];
            }
            let initialValue = configVal !== undefined ? String(configVal) : defaultValue;
            if (configVal === true) initialValue = 'On';
            if (configVal === false) initialValue = 'Off';

            desc.textContent = mapping[initialValue] || mapping['Auto'] || '';

            const dropdown = new Dropdown({
                options: optionsArr,
                defaultValue: initialValue,
                onChange: async (val) => {
                    desc.textContent = mapping[val] || mapping['Auto'] || '';
                    let finalVal = val;
                    if (val === 'On') finalVal = 'On';
                    if (val === 'Off') finalVal = 'Off';
                    if (val === 'Auto') finalVal = 'Auto';
                    if (key === 'n_gpu_layers') finalVal = parseInt(val, 10);

                    await updateBoosterSetting(key, finalVal);
                    if (onValueChange) onValueChange(finalVal);
                }
            });
            dropContainer.appendChild(dropdown.render());
            if (onValueChange) onValueChange(initialValue);
        }
    };

    const makeOptions = (values, labels) => values.map((v, i) => ({ value: String(v), label: labels ? labels[i] : String(v) }));
    const autoOnOff = makeOptions(['Auto', 'On', 'Off']);

    // Initialize Toggles (System state)
    setupToggle('toggle-brain-mode', 'desc-brain-mode', DESCRIPTIONS.brainMode, 'brain_mode', true);
    setupToggle('toggle-lazy-load', 'desc-lazy-load', DESCRIPTIONS.lazyLoad, 'lazy_load_model', true);

    // Booster Settings
    setupCustomDropdown('container-mlock', 'desc-mlock', DESCRIPTIONS.mlock, autoOnOff, 'force_memory_lock', 'Auto');
    setupCustomDropdown('container-booster-profile', 'desc-booster-profile', DESCRIPTIONS.boosterProfile,
        makeOptions(['edge', 'multitasking', 'balance', 'max_boost', 'ultra_max_boost', 'hyper_cluster'],
            ['Edge (Low Power)', 'Multitasking', 'Balanced', 'Max Boost', 'Ultra Max Boost', 'Hyper Cluster']),
        'mode_run', 'balance');
    setupCustomDropdown('container-flash-attn', 'desc-flash-attn', DESCRIPTIONS.flashAttn, autoOnOff, 'flash_attention', 'Auto');
    setupCustomDropdown('container-context', 'desc-context', DESCRIPTIONS.context, makeOptions(['Auto', 'Off', 'Minimal', 'Standard', 'Aggressive', 'Extreme']), 'context_shifting', 'Auto');
    setupCustomDropdown('container-kv-quant', 'desc-kv-quant', DESCRIPTIONS.kvQuant, makeOptions(['Auto', 'Kv16', 'Kv8', 'Kv4']), 'kv_cache_quantization', 'Auto');
    setupCustomDropdown('container-turbo', 'desc-turbo', DESCRIPTIONS.turbo, autoOnOff, 'turbo_quant', 'Auto');
    setupCustomDropdown('container-spec-dec', 'desc-spec-dec', DESCRIPTIONS.specDec, autoOnOff, 'speculative_decoding', 'Auto');
    setupCustomDropdown('container-auto-round', 'desc-auto-round', DESCRIPTIONS.autoRound, autoOnOff, 'auto_round', 'Auto');

    // dflash is an object in rust, let's treat it as string Auto/On/Off if the API accepts it, or just ignore for now if it breaks.
    // Assuming UI maps to 'Auto' 'On' 'Off' properly, we will just pass it to the backend.
    // Wait, DFlashConfig is SmartState<DFlashConfig>. The UI sets it as string 'Auto', 'On', 'Off'. 
    setupCustomDropdown('container-dflash', 'desc-dflash', DESCRIPTIONS.dflash, autoOnOff, 'dflash', 'Auto');
    setupCustomDropdown('container-vram-reclaim', 'desc-vram-reclaim', DESCRIPTIONS.vramReclaim, autoOnOff, 'force_vram_reclaim', 'Auto');
    setupCustomDropdown('container-gpu-layers', 'desc-gpu-layers', DESCRIPTIONS.gpuLayers,
        makeOptions(['-1', '0', '32'], ['GPU (Auto/Full)', 'Only CPU', 'Hybrid']), 'n_gpu_layers', '-1');
    setupCustomDropdown('container-think-mode', 'desc-think-mode', DESCRIPTIONS.thinkMode, autoOnOff, 'think_mode', 'Auto');
    setupCustomDropdown('container-moe', 'desc-moe', DESCRIPTIONS.moe, autoOnOff, 'moe_routing', 'Auto');

    // Chat, Vector, Vision, and Audio Models Selection & Rich Card Rendering
    try {
        let registryMap = {};
        try {
            const registryRes = await fetch(window.getApiBaseUrl() + '/v1/models/installed').catch(() => null);
            if (registryRes && registryRes.ok) {
                const regData = await registryRes.json();
                const rawList = Array.isArray(regData.models) ? regData.models : (Array.isArray(regData.installed) ? regData.installed : []);
                if (rawList.length > 0) {
                    rawList.forEach(m => {
                        if (m && m.id) registryMap[m.id] = m;
                    });
                } else if (regData.installed_models && typeof regData.installed_models === 'object') {
                    registryMap = regData.installed_models;
                }
            }
        } catch (err) {
            console.warn("Could not fetch full registry map:", err);
        }

        let chatOptions = [];
        let vectorOptions = [];
        let visionOptions = [];
        let audioOptions = [];

        let activeChat = permData.active_slots?.chat_slot?.model_id || permData.chat_models?.text || '';
        let activeVector = permData.active_slots?.embed_slot?.model_id || permData.vector_models?.text || '';
        let activeVision = permData.active_slots?.vision_slot?.model_id || permData.vector_models?.vision || '';
        let activeAudio = permData.active_slots?.audio_slot?.model_id || permData.vector_models?.audio || '';

        // Extract models strictly by registry category if available
        let chatModels = permData.available_chat_models || [];
        let vectorModels = permData.available_vector_models || [];
        let visionModels = permData.available_vision_models || [];
        let audioModels = permData.available_audio_models || [];

        if (Object.keys(registryMap).length > 0) {
            chatModels = Object.keys(registryMap).filter(id => registryMap[id].category === 'chat');
            vectorModels = Object.keys(registryMap).filter(id => registryMap[id].category === 'embedding');
            visionModels = Object.keys(registryMap).filter(id => registryMap[id].category === 'vision');
            audioModels = Object.keys(registryMap).filter(id => registryMap[id].category === 'audio');
        }

        if (chatModels.length > 0) chatOptions = makeOptions(chatModels);
        if (activeChat && !chatOptions.find(o => o.value === activeChat)) {
            if (!registryMap[activeChat] || registryMap[activeChat].category === 'chat') {
                chatOptions.unshift({ value: activeChat, label: activeChat });
            }
        }

        if (vectorModels.length > 0) vectorOptions = makeOptions(vectorModels);
        if (activeVector && !vectorOptions.find(o => o.value === activeVector)) {
            if (!registryMap[activeVector] || registryMap[activeVector].category === 'embedding') {
                vectorOptions.unshift({ value: activeVector, label: activeVector });
            }
        }

        if (visionModels.length > 0) visionOptions = makeOptions(visionModels);
        if (activeVision && !visionOptions.find(o => o.value === activeVision)) {
            if (!registryMap[activeVision] || registryMap[activeVision].category === 'vision') {
                visionOptions.unshift({ value: activeVision, label: activeVision });
            }
        }

        if (audioModels.length > 0) audioOptions = makeOptions(audioModels);
        if (activeAudio && !audioOptions.find(o => o.value === activeAudio)) {
            if (!registryMap[activeAudio] || registryMap[activeAudio].category === 'audio') {
                audioOptions.unshift({ value: activeAudio, label: activeAudio });
            }
        }

        chatOptions.unshift({ value: '', label: 'Select Chat Model...' });
        vectorOptions.unshift({ value: '', label: 'Select Vector Model...' });
        visionOptions.unshift({ value: '', label: 'Select Vision Model...' });
        audioOptions.unshift({ value: '', label: 'Select Audio Model...' });

        // Helper function to render Rich Model Card
        const renderModelCard = (cardElementId, selectedModelId) => {
            const cardEl = container.querySelector('#' + cardElementId);
            if (!cardEl) return;

            if (!selectedModelId || !registryMap[selectedModelId]) {
                cardEl.style.display = 'none';
                cardEl.innerHTML = '';
                return;
            }

            const modelData = registryMap[selectedModelId];
            const meta = modelData.metadata || {};
            const format = (modelData.format_type || 'gguf').toUpperCase();
            const bitDepth = meta.bit_depth || 'N/A';
            const quant = meta.quantization || 'Standard';
            const params = meta.parameters || 'Unknown';
            const tasks = modelData.supported_tasks || [];
            
            let contextLabel = 'Context Window';
            let context = meta.context_window || 'Unknown';

            if (modelData.category === 'audio' || tasks.includes('speech_to_text')) {
                contextLabel = 'Audio Window';
                if (!meta.context_window || meta.context_window === 'Unknown') {
                    context = '30s Max';
                }
            } else if (modelData.category === 'vision' && (!meta.context_window || meta.context_window === 'Unknown')) {
                contextLabel = 'Vision Window';
                context = '224x224 PX';
            }

            const hfRepo = modelData.huggingface_repo || (selectedModelId.includes('/') ? selectedModelId : '');
            const extraFiles = Array.isArray(modelData.extra_files) ? modelData.extra_files : [];

            let taskPillsHtml = tasks.map(t => `<span class="task-pill" style="background: rgba(59, 130, 246, 0.15); color: #60a5fa; border: 1px solid rgba(59, 130, 246, 0.3); font-size: 11px; padding: 2px 8px; border-radius: 12px; display: inline-block;">${t}</span>`).join(' ');

            const hfIconHtml = hfRepo ? `
                <a href="https://huggingface.co/${hfRepo}" target="_blank" title="View on HuggingFace Hub (${hfRepo})" style="display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; background: rgba(255, 210, 0, 0.15); border: 1px solid rgba(255, 210, 0, 0.3); border-radius: 6px; text-decoration: none; color: #ffd200; font-size: 13px; transition: transform 0.2s;" onmouseover="this.style.transform='scale(1.1)'" onmouseout="this.style.transform='scale(1.0)'">
                    🤗
                </a>
            ` : '';

            cardEl.style.display = 'block';
            cardEl.innerHTML = `
                <div style="background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(255, 255, 255, 0.08); border-radius: 8px; padding: 14px; margin-top: 6px; width: 100%;">
                    <div style="display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid rgba(255,255,255,0.06); padding-bottom: 8px; margin-bottom: 10px;">
                        <div style="display: flex; align-items: center; gap: 10px;">
                            <span style="background: ${format === 'GGUF' ? 'rgba(16, 185, 129, 0.2)' : 'rgba(168, 85, 247, 0.2)'}; color: ${format === 'GGUF' ? '#10b981' : '#c084fc'}; border: 1px solid ${format === 'GGUF' ? 'rgba(16, 185, 129, 0.3)' : 'rgba(168, 85, 247, 0.3)'}; font-size: 11px; font-weight: 700; padding: 2px 8px; border-radius: 4px;">${format}</span>
                            <span style="font-weight: 600; color: #f3f4f6; font-size: 0.9rem;">${selectedModelId}</span>
                            ${hfIconHtml}
                        </div>
                        <button class="inspect-btn" data-model="${selectedModelId}" style="background: rgba(59, 130, 246, 0.15); border: 1px solid rgba(59, 130, 246, 0.3); color: #60a5fa; font-size: 11px; font-weight: 600; padding: 5px 12px; border-radius: 6px; cursor: pointer; transition: all 0.2s;">🔍 Inspect Files & Header</button>
                    </div>
                    <div style="display: grid; grid-template-columns: repeat(4, 1fr); gap: 8px; margin-bottom: 10px;">
                        <div style="background: rgba(0,0,0,0.2); padding: 6px 10px; border-radius: 4px;">
                            <div style="font-size: 10px; color: #9ca3af;">Parameters</div>
                            <div style="font-size: 12px; font-weight: 600; color: #38bdf8;">${params}</div>
                        </div>
                        <div style="background: rgba(0,0,0,0.2); padding: 6px 10px; border-radius: 4px;">
                            <div style="font-size: 10px; color: #9ca3af;">Bit Precision</div>
                            <div style="font-size: 12px; font-weight: 600; color: #a78bfa;">${bitDepth}</div>
                        </div>
                        <div style="background: rgba(0,0,0,0.2); padding: 6px 10px; border-radius: 4px;">
                            <div style="font-size: 10px; color: #9ca3af;">${contextLabel}</div>
                            <div style="font-size: 12px; font-weight: 600; color: #34d399;">${context}</div>
                        </div>
                        <div style="background: rgba(0,0,0,0.2); padding: 6px 10px; border-radius: 4px;">
                            <div style="font-size: 10px; color: #9ca3af;">Quantization</div>
                            <div style="font-size: 12px; font-weight: 600; color: #fbbf24;">${quant}</div>
                        </div>
                    </div>
                    ${taskPillsHtml ? `<div style="display: flex; gap: 6px; flex-wrap: wrap; align-items: center;"><span style="font-size: 11px; color: #6b7280; margin-right: 4px;">Supported Tasks:</span> ${taskPillsHtml}</div>` : ''}
                </div>
            `;

            // Multi-tab inspector button listener
            const inspectBtn = cardEl.querySelector('.inspect-btn');
            if (inspectBtn) {
                inspectBtn.addEventListener('click', () => {
                    const chatTmpl = meta.chat_template || "No template found.";
                    const localDir = modelData.local_dir || '';
                                    // Clean up tab display titles (remove .json extension, format clean titles)
                    const formatTabName = (filename) => {
                        const clean = filename.replace(/\.json$/, '').replace(/\.yaml$/, '').replace(/\.md$/, '').replace(/_/g, ' ').replace(/-/g, ' ');
                        return clean.charAt(0).toUpperCase() + clean.slice(1);
                    };

                    const hasChatTemplate = chatTmpl && chatTmpl.trim() !== "No template found." && chatTmpl.trim() !== "";
                    
                    showModal(`${selectedModelId}`, `
                        <div style="display: flex; flex-direction: column; height: 100%; width: 100%; text-align: left; gap: 12px;">
                            <!-- Top Scrollable Tab Navigation Bar -->
                            <div class="modal-tab-bar" style="display: flex; gap: 8px; border-bottom: 1px solid rgba(255,255,255,0.1); padding-bottom: 8px; overflow-x: auto; scrollbar-width: none; -ms-overflow-style: none; -webkit-overflow-scrolling: touch; white-space: nowrap;">
                                <style>.modal-tab-bar::-webkit-scrollbar { display: none; }</style>
                                <button class="modal-tab-btn active" data-tab="tab-header" style="background: rgba(59, 130, 246, 0.2); border: 1px solid rgba(59, 130, 246, 0.4); color: #60a5fa; padding: 6px 14px; border-radius: 6px; font-size: 12px; font-weight: 500; cursor: pointer; display: flex; align-items: center; gap: 6px;">⚡ Binary Header Probe</button>
                                <button class="modal-tab-btn" data-tab="tab-raw-header" style="background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; padding: 6px 14px; border-radius: 6px; font-size: 12px; font-weight: 500; cursor: pointer; display: flex; align-items: center; gap: 6px;">🏗️ Inspect Raw Header</button>
                                ${hasChatTemplate ? `<button class="modal-tab-btn" data-tab="tab-template" style="background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; padding: 6px 14px; border-radius: 6px; font-size: 12px; font-weight: 500; cursor: pointer; display: flex; align-items: center; gap: 6px;">💬 Chat Template</button>` : ''}
                                ${extraFiles.map((f, i) => `<button class="modal-tab-btn" data-tab="tab-file-${i}" data-filename="${f}" style="background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; padding: 6px 14px; border-radius: 6px; font-size: 12px; font-weight: 500; cursor: pointer; display: flex; align-items: center; gap: 6px;">📄 ${formatTabName(f)}</button>`).join('')}
                            </div>
                            
                            <!-- Tab Content: Live Binary Header Probe (Active Default) -->
                            <div id="tab-header" class="modal-tab-content" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
                                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
                                    <span style="font-size: 11px; color: #9ca3af; font-family: monospace;">Hardware Probe & Binary Structure Details</span>
                                    <div style="display: flex; gap: 8px;">
                                        <button class="btn-export-tab" data-export-type="header" style="background: rgba(168, 85, 247, 0.2); border: 1px solid rgba(168, 85, 247, 0.4); color: #c084fc; padding: 5px 12px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">📥 Export</button>
                                    </div>
                                </div>
                                <div id="editor-container-header" style="flex: 1; height: 100%; min-height: 0; border-radius: 8px; overflow: hidden; border: 1px solid rgba(255,255,255,0.08);"></div>
                            </div>

                            <!-- Tab Content: Inspect Raw Header -->
                            <div id="tab-raw-header" class="modal-tab-content" style="display: none; flex-direction: column; flex: 1; min-height: 0;">
                                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;">
                                    <span style="font-size: 11px; color: #9ca3af; font-family: monospace;">Zero-load Binary Parse — Raw <span style="background: ${format === 'GGUF' ? 'rgba(16,185,129,0.15)' : 'rgba(168,85,247,0.15)'}; color: ${format === 'GGUF' ? '#10b981' : '#c084fc'}; border: 1px solid ${format === 'GGUF' ? 'rgba(16,185,129,0.3)' : 'rgba(168,85,247,0.3)'}; font-size: 10px; font-weight: 700; padding: 1px 6px; border-radius: 4px;">${format}</span> header read directly from local storage</span>
                                    <div style="display: flex; gap: 8px;">
                                        <button class="btn-tab-search" data-search-target="editor-container-raw-header" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.12); color: #9ca3af; padding: 5px 10px; border-radius: 6px; font-size: 12px; cursor: pointer;" title="Search (Ctrl+F)">🔍</button>
                                        <button id="btn-reload-raw-header" style="background: rgba(34, 197, 94, 0.15); border: 1px solid rgba(34, 197, 94, 0.35); color: #4ade80; padding: 5px 12px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer; display: flex; align-items: center; gap: 5px;">🔄 Reload</button>
                                        <button class="btn-export-tab" data-export-type="raw_header" style="background: rgba(168, 85, 247, 0.2); border: 1px solid rgba(168, 85, 247, 0.4); color: #c084fc; padding: 5px 12px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">📥 Export</button>
                                    </div>
                                </div>
                                <div class="tab-search-bar" style="display: none; align-items: center; gap: 6px; margin-bottom: 6px; background: rgba(10,20,35,0.95); border: 1px solid rgba(96,165,250,0.35); border-radius: 6px; padding: 5px 10px;">
                                    <span style="color:#6b7280; font-size:13px; flex-shrink:0;">🔍</span>
                                    <input class="tab-search-input" type="text" placeholder="Search in header..." style="flex: 1; background: transparent; border: none; outline: none; color: #f3f4f6; font-size: 12px; font-family: monospace; min-width: 0;">
                                    <span class="tab-search-count" style="font-size: 11px; color: #6b7280; min-width: 52px; text-align: center; font-family: monospace;"></span>
                                    <button class="tab-search-exact" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; width: 24px; height: 24px; border-radius: 4px; cursor: pointer; font-size: 12px; font-weight: bold; font-family: serif; display:flex; align-items:center; justify-content:center; margin-right: 2px;" title="Match Case (Exact)">Aa</button>
                                    <button class="tab-search-word" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; width: 24px; height: 24px; border-radius: 4px; cursor: pointer; font-size: 11px; font-weight: bold; font-family: monospace; display:flex; align-items:center; justify-content:center; margin-right: 4px;" title="Match Whole Word"><span style="border-bottom: 1.5px solid currentColor; line-height: 1.1;">ab</span></button>
                                    <button class="tab-search-prev" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; width: 24px; height: 24px; border-radius: 4px; cursor: pointer; font-size: 13px; display:flex; align-items:center; justify-content:center;">↑</button>
                                    <button class="tab-search-next" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; width: 24px; height: 24px; border-radius: 4px; cursor: pointer; font-size: 13px; display:flex; align-items:center; justify-content:center;">↓</button>
                                    <button class="tab-search-close" style="background: none; border: none; color: #6b7280; cursor: pointer; font-size: 18px; padding: 0 2px; line-height: 1;">×</button>
                                </div>
                                <div id="editor-container-raw-header" style="flex: 1; height: 100%; min-height: 0; border-radius: 8px; overflow: hidden; border: 1px solid rgba(255,255,255,0.08);"></div>
                            </div>

                            <!-- Tab Content: Chat Template (Conditional) -->
                            ${hasChatTemplate ? `
                            <div id="tab-template" class="modal-tab-content" style="display: none; flex-direction: column; flex: 1; min-height: 0;">
                                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
                                    <span style="font-size: 11px; color: #9ca3af;">Jinja2 Prompt Formatter template extracted from binary:</span>
                                    <button class="btn-export-tab" data-export-type="template" style="background: rgba(168, 85, 247, 0.2); border: 1px solid rgba(168, 85, 247, 0.4); color: #c084fc; padding: 5px 12px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">📥 Export</button>
                                </div>
                                <div id="editor-container-template" style="flex: 1; height: 100%; min-height: 0; border-radius: 8px; overflow: hidden; border: 1px solid rgba(255,255,255,0.08);"></div>
                            </div>
                            ` : ''}

                            <!-- Dynamic Extra Files Content Slots -->
                            ${extraFiles.map((f, i) => `
                                <div id="tab-file-${i}" class="modal-tab-content" style="display: none; flex-direction: column; flex: 1; min-height: 0;">
                                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
                                        <span style="font-size: 11px; color: #9ca3af; font-family: monospace; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; flex:1; min-width:0;">Path: ${localDir}\\${f}</span>
                                        <div style="display:flex; gap:6px; flex-shrink:0; margin-left:8px;">
                                            <button class="btn-tab-search" data-search-target="editor-container-tab-file-${i}" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.12); color: #9ca3af; padding: 5px 10px; border-radius: 6px; font-size: 12px; cursor: pointer;" title="Search (Ctrl+F)">🔍</button>
                                            <button class="btn-export-tab" data-export-filename="${f}" data-tab-id="tab-file-${i}" style="background: rgba(168, 85, 247, 0.2); border: 1px solid rgba(168, 85, 247, 0.4); color: #c084fc; padding: 5px 12px; border-radius: 6px; font-size: 11px; font-weight: 500; cursor: pointer;">📥 Export</button>
                                        </div>
                                    </div>
                                    <div class="tab-search-bar" style="display: none; align-items: center; gap: 6px; margin-bottom: 6px; background: rgba(10,20,35,0.95); border: 1px solid rgba(96,165,250,0.35); border-radius: 6px; padding: 5px 10px;">
                                        <span style="color:#6b7280; font-size:13px; flex-shrink:0;">🔍</span>
                                        <input class="tab-search-input" type="text" placeholder="Search in file..." style="flex: 1; background: transparent; border: none; outline: none; color: #f3f4f6; font-size: 12px; font-family: monospace; min-width: 0;">
                                        <span class="tab-search-count" style="font-size: 11px; color: #6b7280; min-width: 52px; text-align: center; font-family: monospace;"></span>
                                        <button class="tab-search-exact" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; width: 24px; height: 24px; border-radius: 4px; cursor: pointer; font-size: 12px; font-weight: bold; font-family: serif; display:flex; align-items:center; justify-content:center; margin-right: 2px;" title="Match Case (Exact)">Aa</button>
                                        <button class="tab-search-word" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; width: 24px; height: 24px; border-radius: 4px; cursor: pointer; font-size: 11px; font-weight: bold; font-family: monospace; display:flex; align-items:center; justify-content:center; margin-right: 4px;" title="Match Whole Word"><span style="border-bottom: 1.5px solid currentColor; line-height: 1.1;">ab</span></button>
                                        <button class="tab-search-prev" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; width: 24px; height: 24px; border-radius: 4px; cursor: pointer; font-size: 13px; display:flex; align-items:center; justify-content:center;">↑</button>
                                        <button class="tab-search-next" style="background: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.1); color: #9ca3af; width: 24px; height: 24px; border-radius: 4px; cursor: pointer; font-size: 13px; display:flex; align-items:center; justify-content:center;">↓</button>
                                        <button class="tab-search-close" style="background: none; border: none; color: #6b7280; cursor: pointer; font-size: 18px; padding: 0 2px; line-height: 1;">×</button>
                                    </div>
                                    <div id="editor-container-tab-file-${i}" style="flex: 1; height: 100%; min-height: 0; border-radius: 8px; overflow: hidden; border: 1px solid rgba(255,255,255,0.08);"></div>
                                </div>
                            `).join('')}
                        </div>
                    `, { hideFooter: true });

                    // Tab switcher binding & Auto Header Initialization inside modal
                    setTimeout(() => {
                        const modalOverlay = document.getElementById('hub-global-modal-overlay');
                        if (!modalOverlay) return;
                        
                        const tabBtns = modalOverlay.querySelectorAll('.modal-tab-btn');
                        const tabContents = modalOverlay.querySelectorAll('.modal-tab-content');
                        const editorsMap = {};

                        // Helper to get or create CodeEditor instance inside modal container
                        const getOrCreateEditor = (containerId, initialVal = "", mode = "application/json") => {
                            if (editorsMap[containerId]) return editorsMap[containerId];
                            const containerEl = modalOverlay.querySelector('#' + containerId);
                            if (!containerEl) return null;

                            const editor = new CodeEditor({
                                value: initialVal,
                                mode: mode,
                                readOnly: true,
                                className: 'modal-code-editor'
                            });
                            containerEl.appendChild(editor.render());
                            editor.mount();
                            editorsMap[containerId] = editor;
                            return editor;
                        };
                        
                        // ── Reusable fetch helper for Inspect Raw Header tab ──
                        const fetchAndShowRawHeader = (forceReload) => {
                            const containerId = 'editor-container-raw-header';
                            const containerEl = modalOverlay.querySelector('#' + containerId);
                            if (!containerEl) return;

                            if (forceReload) {
                                // Clear cached editor so we re-fetch fresh
                                delete editorsMap[containerId];
                                containerEl.innerHTML = '';
                            }

                            if (editorsMap[containerId]) {
                                // Already loaded — just refresh
                                const existing = editorsMap[containerId];
                                if (existing && existing.cm) setTimeout(() => existing.cm.refresh(), 20);
                                return;
                            }

                            // Show glowing loader
                            const loaderEl = document.createElement('div');
                            loaderEl.style.cssText = 'position: absolute; inset: 0; background: #090d13; display: flex; flex-direction: column; align-items: center; justify-content: center; z-index: 10; gap: 12px; font-family: sans-serif;';
                            loaderEl.innerHTML = `
                                <style>@keyframes spinRH { 0%{transform:rotate(0deg)} 100%{transform:rotate(360deg)} }</style>
                                <div style="width: 32px; height: 32px; border: 3px solid rgba(96,165,250,0.2); border-top-color: #60a5fa; border-radius: 50%; animation: spinRH 0.8s linear infinite;"></div>
                                <span style="font-size: 12px; color: #9ca3af; font-family: monospace;">Reading GGUF Binary Header from disk...</span>
                            `;
                            containerEl.style.position = 'relative';
                            containerEl.appendChild(loaderEl);

                            (async () => {
                                let ed;
                                try {
                                    const res = await fetch(`/v1/models/${encodeURIComponent(selectedModelId)}/inspect_raw_header`);
                                    if (!res.ok) throw new Error(`HTTP ${res.status}: ${res.statusText}`);
                                    const rawData = await res.json();
                                    ed = getOrCreateEditor(containerId, JSON.stringify(rawData, null, 2), 'application/json');
                                } catch (err) {
                                    ed = getOrCreateEditor(containerId, `// Error: ${err.message}\n// Make sure the API server is running and the model is in the vault.`, 'text/plain');
                                } finally {
                                    if (loaderEl && loaderEl.parentNode) loaderEl.parentNode.removeChild(loaderEl);
                                    if (ed && ed.cm) setTimeout(() => ed.cm.refresh(), 20);
                                }
                            })();
                        };

                        // Wire up the Reload button
                        const reloadRawHeaderBtn = modalOverlay.querySelector('#btn-reload-raw-header');
                        if (reloadRawHeaderBtn) {
                            reloadRawHeaderBtn.addEventListener('click', () => fetchAndShowRawHeader(true));
                        }

                        tabBtns.forEach(btn => {
                            btn.addEventListener('click', async () => {
                                tabBtns.forEach(b => {
                                    b.style.background = 'rgba(255,255,255,0.05)';
                                    b.style.borderColor = 'rgba(255,255,255,0.1)';
                                    b.style.color = '#9ca3af';
                                });
                                tabContents.forEach(c => c.style.display = 'none');
                                
                                btn.style.background = 'rgba(59, 130, 246, 0.2)';
                                btn.style.borderColor = 'rgba(59, 130, 246, 0.4)';
                                btn.style.color = '#60a5fa';
                                
                                const tabId = btn.getAttribute('data-tab');
                                const targetTab = modalOverlay.querySelector('#' + tabId);
                                if (targetTab) targetTab.style.display = 'flex';

                                if (tabId === 'tab-template' && hasChatTemplate) {
                                    const ed = getOrCreateEditor('editor-container-template', chatTmpl, 'jinja2');
                                    if (ed && ed.cm) setTimeout(() => ed.cm.refresh(), 20);
                                } else if (tabId === 'tab-header') {
                                    const ed = getOrCreateEditor('editor-container-header', JSON.stringify(probedHeaderData, null, 2), 'application/json');
                                    if (ed && ed.cm) setTimeout(() => ed.cm.refresh(), 20);
                                } else if (tabId === 'tab-raw-header') {
                                    fetchAndShowRawHeader(false);
                                } else if (tabId && tabId.startsWith('tab-file-')) {
                                    const filename = btn.getAttribute('data-filename');
                                    const containerId = `editor-container-${tabId}`;
                                    const containerEl = modalOverlay.querySelector('#' + containerId);
                                    let ed = editorsMap[containerId];
                                    
                                    if (!ed) {
                                        // Create a high-tech glowing loader overlay inside the container before mount
                                        const loaderOverlay = document.createElement('div');
                                        loaderOverlay.className = 'editor-tab-loader';
                                        loaderOverlay.style.cssText = 'position: absolute; inset: 0; background: #090d13; display: flex; flex-direction: column; align-items: center; justify-content: center; z-index: 10; gap: 12px; font-family: sans-serif;';
                                        loaderOverlay.innerHTML = `
                                            <div style="width: 32px; height: 32px; border: 3px solid rgba(96, 165, 250, 0.2); border-top-color: #60a5fa; border-radius: 50%; animation: spinLoader 0.8s linear infinite;"></div>
                                            <style>@keyframes spinLoader { 0% { transform: rotate(0deg); } 100% { transform: rotate(360deg); } }</style>
                                            <span style="font-size: 12px; color: #9ca3af; font-family: monospace;">Loading ${filename} from disk...</span>
                                        `;
                                        if (containerEl) {
                                            containerEl.style.position = 'relative';
                                            containerEl.appendChild(loaderOverlay);
                                        }

                                        try {
                                            // Non-blocking async fetch
                                            const fileRes = await fetch(`/api/components/file?component_type=model&component_id=${encodeURIComponent(selectedModelId)}&file_path=${encodeURIComponent(filename)}`);
                                            const fileJson = await fileRes.json();
                                            
                                            let content = "";
                                            if (fileJson.status === 'success' && fileJson.content) {
                                                content = fileJson.content;
                                            } else {
                                                content = fileJson.message || "// File content empty or unavailable.";
                                            }

                                            let isTruncated = false;
                                            if (content.length > 500000) {
                                                isTruncated = true;
                                                content = content.substring(0, 500000);
                                            }

                                            // Mount editor smoothly with pure clean JSON
                                            ed = getOrCreateEditor(containerId, content, filename.endsWith('.json') ? 'application/json' : 'text/plain');
                                            
                                        } catch (err) {
                                            ed = getOrCreateEditor(containerId, `// Error loading file: ${err.message}`, 'text/plain');
                                        } finally {
                                            // Remove loader overlay smoothly
                                            if (loaderOverlay && loaderOverlay.parentNode) {
                                                loaderOverlay.parentNode.removeChild(loaderOverlay);
                                            }
                                        }
                                    }
                                    if (ed && ed.cm) setTimeout(() => ed.cm.refresh(), 20);
                                }
                            });
                        });

                        // ── VS Code-style In-tab Search ─────────────────────────────────
                        if (!document.getElementById('cm-search-styles')) {
                            const _ss = document.createElement('style');
                            _ss.id = 'cm-search-styles';
                            _ss.textContent = [
                                '.cm-search-match { background: rgba(255,200,0,0.22); border-radius: 2px; }',
                                '.cm-search-current { background: rgba(255,130,0,0.65); border-radius: 2px; }'
                            ].join('');
                            document.head.appendChild(_ss);
                        }

                        const wireTabSearch = (searchBar, getEditorFn) => {
                            if (searchBar._wired) return;
                            searchBar._wired = true;
                            const input    = searchBar.querySelector('.tab-search-input');
                            const exactBtn = searchBar.querySelector('.tab-search-exact');
                            const wordBtn  = searchBar.querySelector('.tab-search-word');
                            const countEl  = searchBar.querySelector('.tab-search-count');
                            const prevBtn  = searchBar.querySelector('.tab-search-prev');
                            const nextBtn  = searchBar.querySelector('.tab-search-next');
                            const closeBtn = searchBar.querySelector('.tab-search-close');
                            let matches = [], cur = -1, marks = [], debTimer = null;
                            let isExactMatch = false;
                            let isWholeWord = false;
                            let rafId = null;

                            exactBtn.addEventListener('click', () => {
                                isExactMatch = !isExactMatch;
                                exactBtn.style.background = isExactMatch ? 'rgba(59, 130, 246, 0.2)' : 'rgba(255,255,255,0.06)';
                                exactBtn.style.color = isExactMatch ? '#60a5fa' : '#9ca3af';
                                exactBtn.style.borderColor = isExactMatch ? 'rgba(59, 130, 246, 0.4)' : 'rgba(255,255,255,0.1)';
                                doSearch(input.value);
                            });

                            wordBtn.addEventListener('click', () => {
                                isWholeWord = !isWholeWord;
                                wordBtn.style.background = isWholeWord ? 'rgba(59, 130, 246, 0.2)' : 'rgba(255,255,255,0.06)';
                                wordBtn.style.color = isWholeWord ? '#60a5fa' : '#9ca3af';
                                wordBtn.style.borderColor = isWholeWord ? 'rgba(59, 130, 246, 0.4)' : 'rgba(255,255,255,0.1)';
                                doSearch(input.value);
                            });

                            // Clear only the rendered mark window (never all at once)
                            const clearMarks = () => { marks.forEach(m => { try { m.clear(); } catch(_){} }); marks = []; };

                            // Only mark a window of ±40 around current match — avoids marking 10k items
                            const highlight = (idx) => {
                                const ed = getEditorFn(); const cm = ed && ed.cm;
                                if (!cm || !matches.length) return;
                                if (rafId) cancelAnimationFrame(rafId);
                                
                                rafId = requestAnimationFrame(() => {
                                    clearMarks();
                                    const win = 40;
                                    const lo = Math.max(0, idx - win), hi = Math.min(matches.length - 1, idx + win);
                                    for (let i = lo; i <= hi; i++) {
                                        marks.push(cm.markText(matches[i].from, matches[i].to, {
                                            className: i === idx ? 'cm-search-current' : 'cm-search-match'
                                        }));
                                    }
                                    cm.scrollIntoView(matches[idx], 80);
                                    countEl.textContent = `${idx + 1} / ${matches.length}`;
                                    countEl.style.color = '#60a5fa';
                                });
                            };

                            const doSearch = (q) => {
                                const ed = getEditorFn(); const cm = ed && ed.cm;
                                clearMarks(); matches = []; cur = -1;
                                if (!cm || !q || q.length < 1) { countEl.textContent = ''; countEl.style.color = '#6b7280'; return; }
                                // Raw string scan — use RegExp to preserve exact original indices even with case-insensitivity
                                const text = cm.getValue();
                                const escapeRegExp = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                                const flags = (isExactMatch ? 'g' : 'gi') + 'u'; // 'u' flag for Unicode matching
                                let pattern = escapeRegExp(q);
                                if (isWholeWord) {
                                    // True Unicode-aware word boundaries using lookarounds
                                    pattern = `(?<![\\p{L}\\p{N}_])${pattern}(?![\\p{L}\\p{N}_])`;
                                }
                                const regex = new RegExp(pattern, flags);
                                
                                const rawPositions = [];
                                let match;
                                // find all occurrences to show accurate count, up to 100k
                                while ((match = regex.exec(text)) !== null) {
                                    rawPositions.push({ _pos: match.index, _len: match[0].length });
                                    if (rawPositions.length >= 100000) break;
                                }
                                
                                if (!rawPositions.length) {
                                    countEl.textContent = 'No results'; countEl.style.color = '#f87171'; return;
                                }
                                
                                // Convert to lazily resolved objects
                                matches = rawPositions.map(m => ({ _pos: m._pos, _len: m._len, from: null, to: null }));
                                
                                const resolve = (i) => {
                                    if (!matches[i].from) {
                                        matches[i].from = cm.posFromIndex(matches[i]._pos);
                                        matches[i].to   = cm.posFromIndex(matches[i]._pos + matches[i]._len);
                                    }
                                    return matches[i];
                                };
                                
                                const pre = Math.min(80, matches.length);
                                for (let i = 0; i < pre; i++) resolve(i);
                                
                                cur = 0; 
                                resolve(cur); 
                                highlight(cur);
                                matches._resolve = resolve;
                            };

                            let _debTimer = null;
                            input.addEventListener('input', () => {
                                clearTimeout(_debTimer);
                                countEl.textContent = '...'; countEl.style.color = '#6b7280';
                                _debTimer = setTimeout(() => doSearch(input.value), 250);
                            });
                            
                            input.addEventListener('keydown', e => {
                                if (e.key === 'Escape') { closeBtn.click(); return; }
                                if (e.key !== 'Enter' || !matches.length) return;
                                e.preventDefault();
                                cur = e.shiftKey ? (cur - 1 + matches.length) % matches.length : (cur + 1) % matches.length;
                                if (matches._resolve) matches._resolve(cur);
                                highlight(cur);
                            });
                            
                            prevBtn.addEventListener('click', (e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                if (!matches.length) return;
                                cur = (cur - 1 + matches.length) % matches.length;
                                if (matches._resolve) matches._resolve(cur);
                                highlight(cur);
                            });
                            
                            nextBtn.addEventListener('click', (e) => {
                                e.preventDefault();
                                e.stopPropagation();
                                if (!matches.length) return;
                                cur = (cur + 1) % matches.length;
                                if (matches._resolve) matches._resolve(cur);
                                highlight(cur);
                            });
                            
                            closeBtn.addEventListener('click', () => {
                                searchBar.style.display = 'none';
                                input.value = ''; clearMarks(); matches = []; countEl.textContent = ''; clearTimeout(_debTimer);
                                if(rafId) cancelAnimationFrame(rafId);
                            });
                        };

                        const openSearch = (btn) => {
                            const targetId = btn.getAttribute('data-search-target');
                            const tabContent = btn.closest('.modal-tab-content');
                            if (!tabContent) return;
                            const searchBar = tabContent.querySelector('.tab-search-bar');
                            if (!searchBar) return;
                            const isOpen = searchBar.style.display !== 'none';
                            searchBar.style.display = isOpen ? 'none' : 'flex';
                            if (!isOpen) {
                                wireTabSearch(searchBar, () => editorsMap[targetId]);
                                const inp = searchBar.querySelector('.tab-search-input');
                                inp.focus();
                                inp.select();
                            }
                        };

                        modalOverlay.querySelectorAll('.btn-tab-search').forEach(btn => {
                            btn.addEventListener('click', () => openSearch(btn));
                        });

                        // Ctrl+F / Cmd+F shortcut inside the modal
                        modalOverlay.addEventListener('keydown', e => {
                            if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
                                e.preventDefault();
                                const activeTabContent = [...modalOverlay.querySelectorAll('.modal-tab-content')]
                                    .find(el => el.style.display !== 'none');
                                if (!activeTabContent) return;
                                const searchBtn = activeTabContent.querySelector('.btn-tab-search');
                                if (searchBtn) searchBtn.click();
                            }
                        }, true);

                        // Fix for Copy issue in global modal
                        modalOverlay.addEventListener('copy', (e) => {
                            const activeTabContent = [...modalOverlay.querySelectorAll('.modal-tab-content')]
                                .find(el => el.style.display !== 'none');
                            if (!activeTabContent) return;
                            const editorContainer = activeTabContent.querySelector('[id^="editor-container-"]');
                            if (!editorContainer) return;
                            const ed = editorsMap[editorContainer.id];
                            if (ed && ed.cm) {
                                const selectedText = ed.cm.getSelection();
                                if (selectedText) {
                                    e.preventDefault();
                                    e.clipboardData.setData('text/plain', selectedText);
                                }
                            }
                        }, true);

                        // Auto-populate Binary Header Probe CodeEditor immediately on modal open
                        const probedHeaderData = {
                            model_id: selectedModelId,
                            architecture: meta.architecture || "Unknown",
                            format: format,
                            quantization: quant,
                            bit_precision: bitDepth,
                            parameters: params,
                            context_window: context,
                            chat_template_available: hasChatTemplate,
                            storage_path: localDir,
                            files: modelData.files || [],
                            probed_timestamp: new Date().toISOString()
                        };

                        const headerEd = getOrCreateEditor('editor-container-header', JSON.stringify(probedHeaderData, null, 2), 'application/json');
                        if (headerEd && headerEd.cm) setTimeout(() => headerEd.cm.refresh(), 50);

                        // Bind Export Button for ALL Tabs (Format: <model_id>-<file_name>)
                        const exportBtns = modalOverlay.querySelectorAll('.btn-export-tab');
                        exportBtns.forEach(exportBtn => {
                            exportBtn.addEventListener('click', () => {
                                const exportType = exportBtn.getAttribute('data-export-type');
                                const exportFilename = exportBtn.getAttribute('data-export-filename');
                                const tabId = exportBtn.getAttribute('data-tab-id');

                                let contentToExport = "";
                                let downloadName = "";

                                if (exportType === "header") {
                                    contentToExport = JSON.stringify(probedHeaderData, null, 2);
                                    downloadName = `${selectedModelId}-header.json`;
                                } else if (exportType === "raw_header") {
                                    const ed = editorsMap['editor-container-raw-header'];
                                    contentToExport = ed ? ed.getValue() : '';
                                    downloadName = `${selectedModelId}-Inspect-Raw-Header.json`;
                                } else if (exportType === "template") {
                                    contentToExport = chatTmpl;
                                    downloadName = `${selectedModelId}-chat_template.jinja2`;
                                } else if (exportFilename && tabId) {
                                    const containerId = `editor-container-${tabId}`;
                                    const ed = editorsMap[containerId];
                                    if (ed) {
                                        contentToExport = ed.getValue();
                                    }
                                    downloadName = `${selectedModelId}-${exportFilename}`;
                                }

                                if (!contentToExport || contentToExport.startsWith("// Loading")) return;

                                const blob = new Blob([contentToExport], { type: 'text/plain;charset=utf-8' });
                                const url = URL.createObjectURL(blob);
                                const a = document.createElement('a');
                                a.href = url;
                                a.download = downloadName.replace(/\//g, '_');
                                document.body.appendChild(a);
                                a.click();
                                document.body.removeChild(a);
                                URL.revokeObjectURL(url);
                            });
                        });
                    }, 50);
                });
            }
        };

        const chatContainer = container.querySelector('#container-chat-model');
        if (chatContainer) {
            const chatDropdown = new Dropdown({
                options: chatOptions,
                defaultValue: activeChat,
                onChange: async (val) => {
                    console.log('Chat Model changed to:', val);
                    if (!permData.active_slots) permData.active_slots = {};
                    if (!permData.active_slots.chat_slot) permData.active_slots.chat_slot = {};
                    permData.active_slots.chat_slot.model_id = val !== '' ? val : null;
                    await updatePermissionSetting('active_slots', permData.active_slots);
                    renderModelCard('card-chat-model', val);
                }
            });
            chatContainer.appendChild(chatDropdown.render());
            renderModelCard('card-chat-model', activeChat);
        }

        const vectorContainer = container.querySelector('#container-vector-model');
        if (vectorContainer) {
            const vectorDropdown = new Dropdown({
                options: vectorOptions,
                defaultValue: activeVector,
                onChange: async (val) => {
                    console.log('Vector Model changed to:', val);
                    if (!permData.active_slots) permData.active_slots = {};
                    if (!permData.active_slots.embed_slot) permData.active_slots.embed_slot = {};
                    permData.active_slots.embed_slot.model_id = val !== '' ? val : null;
                    await updatePermissionSetting('active_slots', permData.active_slots);
                    renderModelCard('card-vector-model', val);
                }
            });
            vectorContainer.appendChild(vectorDropdown.render());
            renderModelCard('card-vector-model', activeVector);
        }

        const visionContainer = container.querySelector('#container-vision-model');
        if (visionContainer) {
            const visionDropdown = new Dropdown({
                options: visionOptions,
                defaultValue: activeVision,
                onChange: async (val) => {
                    console.log('Vision Model changed to:', val);
                    if (!permData.active_slots) permData.active_slots = {};
                    if (!permData.active_slots.vision_slot) permData.active_slots.vision_slot = {};
                    permData.active_slots.vision_slot.model_id = val !== '' ? val : null;
                    await updatePermissionSetting('active_slots', permData.active_slots);
                    renderModelCard('card-vision-model', val);
                }
            });
            visionContainer.appendChild(visionDropdown.render());
            renderModelCard('card-vision-model', activeVision);
        }

        const audioContainer = container.querySelector('#container-audio-model');
        if (audioContainer) {
            const audioDropdown = new Dropdown({
                options: audioOptions,
                defaultValue: activeAudio,
                onChange: async (val) => {
                    console.log('Audio Model changed to:', val);
                    if (!permData.active_slots) permData.active_slots = {};
                    if (!permData.active_slots.audio_slot) permData.active_slots.audio_slot = {};
                    permData.active_slots.audio_slot.model_id = val !== '' ? val : null;
                    await updatePermissionSetting('active_slots', permData.active_slots);
                    renderModelCard('card-audio-model', val);
                }
            });
            audioContainer.appendChild(audioDropdown.render());
            renderModelCard('card-audio-model', activeAudio);
        }
    } catch (e) {
        console.error('Failed to setup models:', e);
    }

    const btnUnload = container.querySelector('#btn-unload-model');
    if (btnUnload) {
        btnUnload.addEventListener('click', async () => {
            const originalText = btnUnload.innerText;
            btnUnload.innerText = 'Unloading...';
            try {
                const res = await fetch(window.getApiBaseUrl() + '/v1/chat/completions', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        model: "llama",
                        messages: [],
                        keep_alive: 0
                    })
                });

                if (res.ok) {
                    btnUnload.innerText = 'Success!';
                } else {
                    btnUnload.innerText = 'Error!';
                }
            } catch (e) {
                console.error('Failed to unload model:', e);
                btnUnload.innerText = 'Error!';
            }
            setTimeout(() => btnUnload.innerText = originalText, 2000);
        });
    }

    // Tab switching logic for GGUF Metadata Headers
    const tabs = container.querySelectorAll('.booster-tab');
    const contents = container.querySelectorAll('.booster-tab-content');
    tabs.forEach(tab => {
        tab.addEventListener('click', () => {
            tabs.forEach(t => {
                t.classList.remove('active');
                t.style.background = 'transparent';
                t.style.color = '#8b949e';
            });
            contents.forEach(c => c.style.display = 'none');

            tab.classList.add('active');
            tab.style.background = 'rgba(255,255,255,0.1)';
            tab.style.color = 'white';

            const targetId = tab.getAttribute('data-target');
            const targetContent = container.querySelector('#' + targetId);
            if (targetContent) {
                targetContent.style.display = 'block';
            }
        });
    });
}
