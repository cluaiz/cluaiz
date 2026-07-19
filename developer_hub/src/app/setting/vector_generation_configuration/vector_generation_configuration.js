import { Dropdown } from '../../../../components/dropdown/dropdown.js?v=3';

export async function mount(container) {
    let permissionConfig = {};
    let vectorConfig = {
        hardware_and_execution_gguf: { n_gpu_layers: -1, batch_size: 512 },
        hardware_and_execution_onnx: { execution_provider: "CUDA" },
        processing: { normalize: true, pooling_strategy: "Mean" }
    };
    let models = [];

    // Load initial configs
    try {
        const pRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission').catch(() => null);
        if (pRes && pRes.ok) {
            permissionConfig = await pRes.json();
            if (!permissionConfig.vector_models) permissionConfig.vector_models = { text: null, vision: null, audio: null };
        }

        const vRes = await fetch(window.getApiBaseUrl() + '/v1/system/vector_config').catch(() => null);
        if (vRes && vRes.ok) {
            const data = await vRes.json();
            if (data) vectorConfig = { ...vectorConfig, ...data };
        }

        const mRes = await fetch(window.getApiBaseUrl() + '/v1/models/installed').catch(() => null);
        if (mRes && mRes.ok) {
            models = (await mRes.json()).models || [];
        }
    } catch (e) {
        console.error("Failed to load vector configs or models", e);
    }

    const savePermission = async () => {
        try {
            await fetch(window.getApiBaseUrl() + '/v1/system/permission', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(permissionConfig)
            });
        } catch (e) { console.error("Auto-save Permission failed", e); }
    };

    const saveVectorConfig = async () => {
        try {
            await fetch(window.getApiBaseUrl() + '/v1/system/vector_config', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(vectorConfig)
            });
        } catch (e) { console.error("Auto-save Vector Config failed", e); }
    };

    // ----- UI Helpers -----

    const setupCustomDropdown = (containerId, optionsArr, configObj, section, key, onSave) => {
        const dropContainer = container.querySelector('#' + containerId);
        if (dropContainer) {
            let configVal;
            if (section) {
                if (!configObj[section]) configObj[section] = {};
                configVal = configObj[section][key];
            } else {
                configVal = configObj[key];
            }

            let initialValue = configVal !== undefined && configVal !== null ? String(configVal) : optionsArr[0].value;

            const dropdown = new Dropdown({
                options: optionsArr,
                defaultValue: initialValue,
                onChange: async (val) => {
                    let finalVal = val;
                    if (val === 'true') finalVal = true;
                    if (val === 'false') finalVal = false;
                    if (!isNaN(val) && val !== '' && val !== null) {
                        const num = Number(val);
                        if (String(num) === val) finalVal = num;
                    }

                    if (section) configObj[section][key] = finalVal;
                    else configObj[key] = finalVal;

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
                const isActive = toggle.classList.toggle('active');
                if (section) configObj[section][key] = isActive;
                else configObj[key] = isActive;
                await onSave();
            });
        }
    };

    const makeOptions = (values, labels) => values.map((v, i) => ({ value: String(v), label: labels ? labels[i] : String(v) }));

    // ----- INITIALIZATION -----
    
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

    // GGUF Hardware
    const vectorCustomDropdownOptions = [
        { value: '-1', label: 'Full GPU (Auto)' },
        { value: '0', label: 'CPU Only' },
        { isInput: true, placeholder: 'Hybrid Layers (e.g. 16)', suffix: 'Layers', inputType: 'number' }
    ];

    setupCustomDropdown('container-vector-gguf-gpu-layers', 
        vectorCustomDropdownOptions,
        vectorConfig, 'hardware_and_execution_gguf', 'n_gpu_layers', saveVectorConfig);
    
    setupCustomDropdown('container-vector-gguf-batch-size', 
        makeOptions(['128', '256', '512', '1024', '2048']),
        vectorConfig, 'hardware_and_execution_gguf', 'batch_size', saveVectorConfig);

    // ONNX Hardware
    let vectorOnnxHardwareOptions = isAppleSilicon ? 
        makeOptions(['Auto'], ['Auto (Apple Silicon)']) : 
        makeOptions(['Auto', 'CPU', 'GPU']);

    setupCustomDropdown('container-vector-onnx-provider', 
        vectorOnnxHardwareOptions,
        vectorConfig, 'hardware_and_execution_onnx', 'hardware_offload', saveVectorConfig);

    // Processing
    setupToggle('toggle-vector-normalize', vectorConfig, 'processing', 'normalize', saveVectorConfig);
    
    setupCustomDropdown('container-vector-pooling', 
        makeOptions(['Mean', 'CLS', 'Max']),
        vectorConfig, 'processing', 'pooling_strategy', saveVectorConfig);
}
