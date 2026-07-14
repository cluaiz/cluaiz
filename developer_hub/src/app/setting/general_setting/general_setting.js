import { Dropdown } from '../../../../components/dropdown/dropdown.js?v=2';

export function mount(container) {
    // Setup toggle logic
    const toggles = container.querySelectorAll('.setting-toggle');
    toggles.forEach(toggle => {
        toggle.addEventListener('click', (e) => {
            e.currentTarget.classList.toggle('active');
            // Here you would save the setting state
        });
    });

    // Initialize Custom Dropdown for Language
    const langContainer = container.querySelector('#container-language');
    if (langContainer) {
        const langDropdown = new Dropdown({
            options: [
                { value: 'en', label: 'English (US)' },
                { value: 'hi', label: 'Hindi (भारत)' },
                { value: 'es', label: 'Spanish' },
                { value: 'fr', label: 'French' }
            ],
            defaultValue: 'en',
            onChange: (val) => {
                console.log(`Language changed to: ${val}`);
                // TODO: Save to backend/store here
            }
        });
        langContainer.appendChild(langDropdown.render());
    }

    // Modal Logic
    const modal = document.getElementById('restart-modal-overlay');
    const btnCancel = document.getElementById('btn-cancel-restart');
    const btnConfirm = document.getElementById('btn-confirm-restart');

    function showRestartModal() {
        if (modal) {
            modal.style.display = 'flex';
            // slight delay to allow display block to apply before opacity transition
            setTimeout(() => {
                modal.classList.add('show');
            }, 10);
        }
    }

    function hideRestartModal() {
        if (modal) {
            modal.classList.remove('show');
            setTimeout(() => {
                modal.style.display = 'none';
            }, 300);
        }
    }

    if (btnCancel) {
        btnCancel.addEventListener('click', hideRestartModal);
    }

    if (btnConfirm) {
        btnConfirm.addEventListener('click', () => {
            // Since we can't restart the backend from the UI, just tell the user to do it manually
            btnConfirm.innerText = "Please restart in terminal, then click here to Reload";
            
            // If they click it again after we changed the text, just reload the page
            if (btnConfirm.dataset.readyToReload) {
                window.location.reload();
            } else {
                btnConfirm.dataset.readyToReload = "true";
            }
        });
    }
    // Initialize Dropdowns Dynamically from Backend
    const currentApiUrl = window.location.origin;
    
    fetch(currentApiUrl + '/v1/system/permission')
        .then(res => res.json())
        .then(data => {
            const schema = data.permission;
            const savedProtocol = schema.connection_protocol || 'http';
            const savedPort = (schema.api_port || 8000).toString();

            // Initialize Connection Protocol Dropdown
            const connContainer = container.querySelector('#container-conn-protocol');
            if (connContainer) {
                const connDropdown = new Dropdown({
                    options: [
                        { value: 'http', label: 'HTTP REST API (Default)' },
                        { value: 'ffi', label: 'Native C-Pointer (FFI)' }
                    ],
                    defaultValue: savedProtocol,
                    onChange: async (val) => {
                        console.log(`Connection protocol changed to: ${val}`);
                        try {
                            schema.connection_protocol = val;
                            await fetch(currentApiUrl + '/v1/system/permission', {
                                method: 'POST',
                                headers: { 'Content-Type': 'application/json' },
                                body: JSON.stringify(schema)
                            });
                            console.log("Connection Protocol updated in backend config.");
                        } catch (e) {
                            console.error("Failed to update backend connection protocol:", e);
                        }
                        showRestartModal();
                    }
                });
                connContainer.appendChild(connDropdown.render());
            }

            // Initialize Localhost Port Dropdown
            const portContainer = container.querySelector('#container-local-port');
            if (portContainer) {
                const portDropdown = new Dropdown({
                    options: [
                        { value: '8000', label: 'Port 8000 (Default)' },
                        { value: '8080', label: 'Port 8080' },
                        { value: '9000', label: 'Port 9000' },
                        { value: '1420', label: 'Port 1420' }
                    ],
                    defaultValue: savedPort,
                    onChange: async (val) => {
                        console.log(`Localhost port changed to: ${val}`);
                        try {
                            schema.api_port = parseInt(val, 10);
                            await fetch(currentApiUrl + '/v1/system/permission', {
                                method: 'POST',
                                headers: { 'Content-Type': 'application/json' },
                                body: JSON.stringify(schema)
                            });
                            console.log("Port updated in backend config.");
                        } catch (e) {
                            console.error("Failed to update backend port:", e);
                        }
                        showRestartModal();
                    }
                });
                portContainer.appendChild(portDropdown.render());
            }
        })
        .catch(err => {
            console.error("Failed to load backend permissions for settings UI", err);
        });
}
