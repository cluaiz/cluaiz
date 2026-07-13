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
            // Simulate full system restart by reloading the Developer Hub UI for now
            console.log("Restarting system...");
            btnConfirm.innerText = "Restarting...";
            btnConfirm.style.opacity = "0.7";
            setTimeout(() => {
                window.location.reload();
            }, 1000);
        });
    }

    // Initialize Connection Protocol Dropdown
    const connContainer = container.querySelector('#container-conn-protocol');
    if (connContainer) {
        const savedProtocol = localStorage.getItem('cluaiz_connection_protocol') || 'http';
        const connDropdown = new Dropdown({
            options: [
                { value: 'http', label: 'HTTP REST API (Default)' },
                { value: 'ffi', label: 'Native C-Pointer (FFI)' }
            ],
            defaultValue: savedProtocol,
            onChange: (val) => {
                console.log(`Connection protocol changed to: ${val}`);
                localStorage.setItem('cluaiz_connection_protocol', val);
                showRestartModal();
            }
        });
        connContainer.appendChild(connDropdown.render());
    }

    // Initialize Localhost Port Dropdown
    const portContainer = container.querySelector('#container-local-port');
    if (portContainer) {
        const savedPort = localStorage.getItem('cluaiz_localhost_port') || '8000';
        const portDropdown = new Dropdown({
            options: [
                { value: '8000', label: 'Port 8000 (Default)' },
                { value: '8080', label: 'Port 8080' },
                { value: '9000', label: 'Port 9000' },
                { value: '1420', label: 'Port 1420' }
            ],
            defaultValue: savedPort,
            onChange: (val) => {
                console.log(`Localhost port changed to: ${val}`);
                localStorage.setItem('cluaiz_localhost_port', val);
                showRestartModal();
            }
        });
        portContainer.appendChild(portDropdown.render());
    }
}
