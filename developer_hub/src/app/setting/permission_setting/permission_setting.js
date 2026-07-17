import { Dropdown } from '../../../../components/dropdown/dropdown.js?v=3';
import { showModal } from '../../../../components/modal/modal.js';

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
    modelHeader: {
        true: 'Enabled: Injects model name and type tags (e.g. <cluaiz_model_name>) directly into the chat SSE stream for client apps to parse.',
        false: 'Disabled: The chat stream will only contain raw generated text without any model metadata headers.'
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
        if (toggle) {
            let isActive = permData[key] === true;
            if (isActive) toggle.classList.add('active');
            else toggle.classList.remove('active');
            
            const desc = descId ? container.querySelector('#' + descId) : null;
            if (desc && mapping) desc.textContent = mapping[isActive] || '';

            toggle.addEventListener('click', async () => {
                toggle.classList.toggle('active');
                const newState = toggle.classList.contains('active');
                if (desc && mapping) desc.textContent = mapping[newState] || '';
                await updatePermissionSetting(key, newState);
            });
        }
    };

    const makeOptions = (values, labels) => values.map((v, i) => ({ value: String(v), label: labels ? labels[i] : String(v) }));

    // Initialize API Auth state
    if (!permData.api_auth) {
        permData.api_auth = { required: false, tokens: [] };
    }

    // Initialize Security & SSO Toggles (dummy logic for these if not in schema, but updating permData)
    setupToggle('toggle-sso-boot', null, null, 'require_login_on_boot');
    setupToggle('toggle-auto-shell', null, null, 'auto_execute_shell');
    setupToggle('toggle-fs-read', null, null, 'workspace_read_access');
    
    // Custom setup for api_auth toggle
    const authToggle = container.querySelector('#toggle-api-auth');
    if (authToggle) {
        let isAuthRequired = permData.api_auth.required === true;
        if (isAuthRequired) authToggle.classList.add('active');
        else authToggle.classList.remove('active');
        
        authToggle.addEventListener('click', async () => {
            const isCurrentlyActive = authToggle.classList.contains('active');
            const action = isCurrentlyActive ? 'Disable' : 'Enable';
            const message = isCurrentlyActive 
                ? 'Are you sure you want to disable API Authentication? The engine will accept requests without a Bearer token.'
                : 'Are you sure you want to enable API Authentication? All HTTP REST requests will require a valid Bearer token.';
            
            const confirmed = await showDialog(`${action} API Authentication`, message, true);
            
            if (confirmed) {
                authToggle.classList.toggle('active');
                permData.api_auth.required = authToggle.classList.contains('active');
                await updatePermissionSetting('api_auth', permData.api_auth);
            }
        });
    }

    const selectKeystore = container.querySelector('#select-keystore');
    if (selectKeystore) {
        if (permData.api_key_storage) selectKeystore.value = permData.api_key_storage;
        selectKeystore.addEventListener('change', async (e) => {
            await updatePermissionSetting('api_key_storage', e.target.value);
        });
    }

    // Initialize API Keys (Bearer Tokens)
    const btnGenerateToken = container.querySelector('#btn-generate-token');
    const apiKeysList = container.querySelector('#api-keys-list');

    // Use the imported reusable modal
    const showDialog = async (titleText, messageHTML, showCancel = false) => {
        return await showModal(titleText, messageHTML, { showCancel: showCancel });
    };

    const renderApiKeys = () => {
        if (!apiKeysList) return;
        apiKeysList.innerHTML = '';
        const tokens = permData.api_auth.tokens || [];
        
        if (tokens.length === 0) {
            apiKeysList.innerHTML = '<div style="color: #8b949e; font-size: 0.9rem; padding: 4px 0;">No API keys generated yet.</div>';
            if (btnGenerateToken) {
                const btnText = btnGenerateToken.querySelector('#btn-generate-text');
                if (btnText) btnText.textContent = 'Generate Key';
            }
            return;
        }

        if (btnGenerateToken) {
            const btnText = btnGenerateToken.querySelector('#btn-generate-text');
            if (btnText) btnText.textContent = 'Regenerate Key';
        }

        tokens.forEach((token, idx) => {
            const tokenRow = document.createElement('div');
            tokenRow.style.cssText = 'display: flex; justify-content: space-between; align-items: center; background: var(--bg-secondary); padding: 8px 12px; border-radius: 6px; border: 1px solid var(--border-color);';
            
            const tokenText = document.createElement('span');
            tokenText.style.cssText = 'font-family: monospace; color: var(--text-primary); font-size: 0.9rem; letter-spacing: 0.5px;';
            tokenText.textContent = token;

            const actions = document.createElement('div');
            actions.style.cssText = 'display: flex; gap: 8px;';

            const copyBtn = document.createElement('button');
            copyBtn.className = 'icon-btn hover-text-primary';
            copyBtn.innerHTML = '<i data-lucide="copy" class="w-4 h-4"></i>';
            copyBtn.title = 'Copy Token';
            copyBtn.onclick = () => {
                navigator.clipboard.writeText(token);
                copyBtn.innerHTML = '<i data-lucide="check" class="w-4 h-4" style="color: var(--method-get)"></i>';
                if (window.lucide) window.lucide.createIcons();
                setTimeout(() => {
                    copyBtn.innerHTML = '<i data-lucide="copy" class="w-4 h-4"></i>';
                    if (window.lucide) window.lucide.createIcons();
                }, 2000);
            };

            const revokeBtn = document.createElement('button');
            revokeBtn.className = 'icon-btn';
            revokeBtn.style.color = 'var(--method-delete)';
            revokeBtn.innerHTML = '<i data-lucide="trash-2" class="w-4 h-4"></i>';
            revokeBtn.title = 'Revoke Token';
            revokeBtn.onclick = async () => {
                const confirmed = await showDialog('Revoke Key', `Are you sure you want to revoke key <b>...${token.slice(-6)}</b>? This will break any integration using it.`, true);
                if (confirmed) {
                    permData.api_auth.tokens = permData.api_auth.tokens.filter(t => t !== token);
                    await updatePermissionSetting('api_auth', permData.api_auth);
                    renderApiKeys();
                }
            };

            actions.appendChild(copyBtn);
            actions.appendChild(revokeBtn);
            tokenRow.appendChild(tokenText);
            tokenRow.appendChild(actions);
            apiKeysList.appendChild(tokenRow);
        });
        // Re-initialize lucide icons for dynamically added elements if available
        if (window.lucide) window.lucide.createIcons();
    };

    renderApiKeys();

    if (btnGenerateToken) {
        btnGenerateToken.addEventListener('click', async () => {
            let confirmed = true;
            if (permData.api_auth.tokens && permData.api_auth.tokens.length > 0) {
                confirmed = await showDialog('Regenerate Key', 'Are you sure you want to regenerate the API key? The old key will immediately stop working and applications using it will lose access.', true);
            }
            
            if (confirmed) {
                const newToken = 'sk-cluaiz-' + Array.from(crypto.getRandomValues(new Uint8Array(16)))
                    .map(b => b.toString(16).padStart(2, '0')).join('');
                
                // Enforce single key only
                permData.api_auth.tokens = [newToken];
                
                await updatePermissionSetting('api_auth', permData.api_auth);
                renderApiKeys();
            }
        });
    }

    // Initialize Engine Security & Privacy Toggles
    setupToggle('toggle-telemetry', 'desc-telemetry', DESCRIPTIONS.telemetry, 'stream_telemetry');
    setupToggle('toggle-model-header', 'desc-model-header', DESCRIPTIONS.modelHeader, 'model_header_info');

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
}
