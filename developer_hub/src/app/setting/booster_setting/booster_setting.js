import { Dropdown } from '../../../../components/dropdown/dropdown.js?v=3';

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
        'separated': 'The reasoning process is parsed and cleanly separated from the final answer.',
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
        } catch(e) {
            console.error("Failed to update booster setting:", e);
        }
    };

    const updatePermissionSetting = async (key, value) => {
        try {
            permData[key] = value;
            await fetch(window.getApiBaseUrl() + '/v1/system/permission', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(permData)
            });
        } catch(e) {
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
    
    setupCustomDropdown('container-vram-reclaim', 'desc-vram-reclaim', DESCRIPTIONS.vramReclaim, autoOnOff, 'force_vram_reclaim', 'Auto');
    setupCustomDropdown('container-gpu-layers', 'desc-gpu-layers', DESCRIPTIONS.gpuLayers, 
        makeOptions(['-1', '0', '32'], ['GPU (Auto/Full)', 'Only CPU', 'Hybrid']), 'n_gpu_layers', '-1');
    setupCustomDropdown('container-output-style', 'desc-output-style', DESCRIPTIONS.outputStyle, makeOptions(['separated', 'raw'], ['Separated (Clean)', 'Raw (With Tags)']), 'ai_response_format.output_style', 'separated');

    const outputStyleItem = container.querySelector('#item-output-style');
    setupCustomDropdown('container-think-mode', 'desc-think-mode', DESCRIPTIONS.thinkMode, autoOnOff, 'ai_response_format.think_mode', 'Auto', (val) => {
        if (outputStyleItem) {
            if (val === 'On') {
                outputStyleItem.style.display = 'flex';
            } else {
                outputStyleItem.style.display = 'none';
            }
        }
    });
    setupCustomDropdown('container-moe', 'desc-moe', DESCRIPTIONS.moe, autoOnOff, 'moe_routing', 'Auto');

    // Chat and Vector Models
    try {
        let chatOptions = [{ value: 'llama3:8b', label: 'llama3:8b' }];
        let vectorOptions = [{ value: 'all-minilm-l6-v2', label: 'all-minilm-l6-v2' }];
        let activeChat = 'llama3:8b';
        let activeVector = 'all-minilm-l6-v2';

        const chatModels = installedModels.filter(m => m.category === 'chat').map(m => m.id);
        if (chatModels.length > 0) {
            chatOptions = makeOptions(chatModels);
            activeChat = chatModels[0];
        }
        
        const vectorModels = installedModels.filter(m => m.category === 'vector').map(m => m.id);
        if (vectorModels.length > 0) {
            vectorOptions = makeOptions(vectorModels);
            activeVector = vectorModels[0];
        }

        if (permData.chat_models?.text) {
            activeChat = permData.chat_models.text;
            if (!chatOptions.find(o => o.value === activeChat)) {
                chatOptions.unshift({ value: activeChat, label: activeChat });
            }
        }
        if (permData.vector_models?.text) {
            activeVector = permData.vector_models.text;
            if (!vectorOptions.find(o => o.value === activeVector)) {
                vectorOptions.unshift({ value: activeVector, label: activeVector });
            }
        }

        const chatContainer = container.querySelector('#container-chat-model');
        if (chatContainer) {
            const chatDropdown = new Dropdown({
                options: chatOptions,
                defaultValue: activeChat,
                onChange: async (val) => {
                    console.log('Chat Model changed to:', val);
                    if (!permData.chat_models) permData.chat_models = {};
                    permData.chat_models.text = val;
                    await updatePermissionSetting('chat_models', permData.chat_models);
                }
            });
            chatContainer.appendChild(chatDropdown.render());
        }

        const vectorContainer = container.querySelector('#container-vector-model');
        if (vectorContainer) {
            const vectorDropdown = new Dropdown({
                options: vectorOptions,
                defaultValue: activeVector,
                onChange: async (val) => {
                    console.log('Vector Model changed to:', val);
                    if (!permData.vector_models) permData.vector_models = {};
                    permData.vector_models.text = val;
                    await updatePermissionSetting('vector_models', permData.vector_models);
                }
            });
            vectorContainer.appendChild(vectorDropdown.render());
        }
    } catch (e) {
        console.error('Failed to setup models:', e);
    }
}
