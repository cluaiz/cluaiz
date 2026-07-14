import { Dropdown } from '../../../../components/dropdown/dropdown.js?v=3';

const DESCRIPTIONS = {
    wasmFirewall: {
        'auto': 'Blocks dangerous plugins automatically based on heuristics.',
        'strict': 'Maximum security. All WASM plugins are heavily restricted.',
        'off': 'No restrictions. Use only with trusted plugins.'
    },
    telemetry: {
        true: 'Enabled: Sending anonymous performance data.',
        false: 'Disabled: No data leaves your machine.'
    },
    vecUser: {
        true: 'Enabled: Your inputs are vectorized and stored in semantic memory.',
        false: 'Disabled: Your inputs are not saved to semantic memory.'
    },
    vecAi: {
        true: 'Enabled: AI responses are vectorized and stored in semantic memory.',
        false: 'Disabled: AI responses are not saved to semantic memory.'
    },
    kvCache: {
        true: 'Enabled: Allows models to cache conversation state in memory for faster responses.',
        false: 'Disabled: Models process the entire conversation history from scratch every time.'
    },
    chatTtl: {
        '1': '1 Hour',
        '12': '12 Hours',
        '24': '24 Hours',
        '72': '72 Hours',
        '168': '1 Week',
        '720': '1 Month'
    }
};

export async function mount(container) {
    let permData = {};

    try {
        const permRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
        if (permRes.ok) {
            const data = await permRes.json();
            permData = data.permission || {};
        }
    } catch (e) {
        console.error("Failed to load permission settings:", e);
    }

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

    const setupToggle = (id, descId, mapping, key) => {
        const toggle = container.querySelector('#' + id);
        const desc = container.querySelector('#' + descId);
        if (toggle && desc) {
            let isActive = permData[key] === true;
            if (isActive) toggle.classList.add('active');
            else toggle.classList.remove('active');
            
            if (mapping) desc.textContent = mapping[isActive] || '';

            toggle.addEventListener('click', async () => {
                toggle.classList.toggle('active');
                const newState = toggle.classList.contains('active');
                if (mapping) desc.textContent = mapping[newState] || '';
                await updatePermissionSetting(key, newState);
            });
        }
    };

    const makeOptions = (values, labels) => values.map((v, i) => ({ value: String(v), label: labels ? labels[i] : String(v) }));

    // Initialize Security & SSO Toggles (dummy logic for these if not in schema, but updating permData)
    setupToggle('toggle-sso-boot', null, null, 'require_login_on_boot');
    setupToggle('toggle-auto-shell', null, null, 'auto_execute_shell');
    setupToggle('toggle-fs-read', null, null, 'workspace_read_access');

    const selectKeystore = container.querySelector('#select-keystore');
    if (selectKeystore) {
        if (permData.api_key_storage) selectKeystore.value = permData.api_key_storage;
        selectKeystore.addEventListener('change', async (e) => {
            await updatePermissionSetting('api_key_storage', e.target.value);
        });
    }

    // Initialize Engine Security & Privacy Toggles
    setupToggle('toggle-telemetry', 'desc-telemetry', DESCRIPTIONS.telemetry, 'stream_telemetry');

    // Initialize WASM Firewall Dropdown
    const wasmContainer = container.querySelector('#container-wasm-firewall');
    const wasmDesc = container.querySelector('#desc-wasm-firewall');
    if (wasmContainer && wasmDesc) {
        let initialVal = permData.wasm_firewall || 'auto';
        wasmDesc.textContent = DESCRIPTIONS.wasmFirewall[initialVal];
        const wasmDropdown = new Dropdown({
            options: makeOptions(['auto', 'strict', 'off'], ['Auto', 'Strict', 'Off']),
            defaultValue: initialVal,
            onChange: async (val) => {
                wasmDesc.textContent = DESCRIPTIONS.wasmFirewall[val] || '';
                await updatePermissionSetting('wasm_firewall', val);
            }
        });
        wasmContainer.appendChild(wasmDropdown.render());
    }

    // Initialize Context & Memory Permissions Toggles
    setupToggle('toggle-vec-user', 'desc-vec-user', DESCRIPTIONS.vecUser, 'vectorize_user_input');
    setupToggle('toggle-vec-ai', 'desc-vec-ai', DESCRIPTIONS.vecAi, 'vectorize_ai_response');
    setupToggle('toggle-kv-cache', 'desc-kv-cache', DESCRIPTIONS.kvCache, 'enable_kvcache');

    // Initialize Chat TTL Dropdown
    const ttlContainer = container.querySelector('#container-chat-ttl');
    const ttlDesc = container.querySelector('#desc-chat-ttl');
    if (ttlContainer && ttlDesc) {
        let initialVal = String(permData.temporary_chat_ttl_hours || '24');
        ttlDesc.textContent = DESCRIPTIONS.chatTtl[initialVal] || '';
        const ttlDropdown = new Dropdown({
            options: makeOptions(['1', '12', '24', '72', '168', '720'], ['1 Hour', '12 Hours', '24 Hours', '72 Hours', '1 Week', '1 Month']),
            defaultValue: initialVal,
            onChange: async (val) => {
                ttlDesc.textContent = DESCRIPTIONS.chatTtl[val] || '';
                await updatePermissionSetting('temporary_chat_ttl_hours', parseInt(val, 10));
            }
        });
        ttlContainer.appendChild(ttlDropdown.render());
    }
}
