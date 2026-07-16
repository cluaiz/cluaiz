const injectStyles = () => {
    const cssId = 'modal-component-css';
    if (!document.getElementById(cssId)) {
        const link = document.createElement('link');
        link.id = cssId;
        link.rel = 'stylesheet';
        // Ensure relative path from developer_hub/src/app... back to components works,
        // or just use absolute path from web root
        link.href = '/components/modal/modal.css';
        document.head.appendChild(link);
    }
};

/**
 * Reusable Custom Modal Component
 * 
 * Injects modal HTML if it doesn't exist and displays it with animation.
 * Returns a Promise that resolves when the user interacts with the modal.
 * 
 * @param {string} title - The title of the modal.
 * @param {string} message - The main text of the modal.
 * @param {Object} [options] - Configuration options.
 * @param {string} [options.confirmText="Confirm"] - Text for the primary button.
 * @param {string} [options.cancelText="Cancel"] - Text for the secondary button.
 * @param {boolean} [options.showCancel=true] - Whether to show the cancel button.
 * @returns {Promise<boolean>} - Resolves to true if confirmed, false if canceled.
 */
export function showModal(title, message, options = {}) {
    return new Promise((resolve) => {
        const config = {
            confirmText: options.confirmText || 'Confirm',
            cancelText: options.cancelText || 'Cancel',
            showCancel: options.showCancel !== false,
        };

        let overlay = document.getElementById('hub-global-modal-overlay');
        
        injectStyles();
        
        // Create modal DOM if it doesn't exist
        if (!overlay) {
            overlay = document.createElement('div');
            overlay.id = 'hub-global-modal-overlay';
            overlay.className = 'modal-overlay';
            
            overlay.innerHTML = `
                <div class="modal-content glass-panel">
                    <h3 class="modal-title" id="hub-global-modal-title"></h3>
                    <p class="modal-text" id="hub-global-modal-message"></p>
                    <div class="modal-actions" id="hub-global-modal-actions">
                        <button id="hub-global-modal-cancel" class="btn-secondary"></button>
                        <button id="hub-global-modal-confirm" class="btn-primary"></button>
                    </div>
                </div>
            `;
            
            document.body.appendChild(overlay);
        }

        const titleEl = document.getElementById('hub-global-modal-title');
        const messageEl = document.getElementById('hub-global-modal-message');
        const cancelBtn = document.getElementById('hub-global-modal-cancel');
        const confirmBtn = document.getElementById('hub-global-modal-confirm');

        titleEl.textContent = title;
        messageEl.innerHTML = message;
        
        confirmBtn.textContent = config.confirmText;
        cancelBtn.textContent = config.cancelText;
        cancelBtn.style.display = config.showCancel ? 'inline-block' : 'none';

        // Event handler cleanup
        const cleanup = () => {
            overlay.classList.remove('show');
            setTimeout(() => {
                overlay.style.display = 'none';
            }, 300); // Wait for transition
        };

        const handleConfirm = () => {
            cleanup();
            confirmBtn.removeEventListener('click', handleConfirm);
            cancelBtn.removeEventListener('click', handleCancel);
            resolve(true);
        };

        const handleCancel = () => {
            cleanup();
            confirmBtn.removeEventListener('click', handleConfirm);
            cancelBtn.removeEventListener('click', handleCancel);
            resolve(false);
        };

        // Attach listeners
        confirmBtn.addEventListener('click', handleConfirm);
        cancelBtn.addEventListener('click', handleCancel);

        // Show modal
        overlay.style.display = 'flex';
        // Force reflow for transition
        overlay.offsetHeight; 
        overlay.classList.add('show');
    });
}
