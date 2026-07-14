export async function mount(container) {
    const listContainer = container.querySelector('#tools-list');
    const modal = container.querySelector('#tool-editor-modal');
    const textarea = container.querySelector('#tool-editor-textarea');
    const title = container.querySelector('#tool-editor-title');
    const closeBtn = container.querySelector('#tool-editor-close');
    const tabBtns = container.querySelectorAll('.setting-tab-btn');
    const fileTree = container.querySelector('#tool-file-tree');
    const fileViewerTitle = container.querySelector('#file-viewer-title');
    const btnClearTemp = container.querySelector('#btn-clear-temp');
    const btnClearAll = container.querySelector('#btn-clear-all');
    const btnUninstall = container.querySelector('#btn-uninstall');
    const modalToggle = container.querySelector('#modal-tool-toggle');

    let currentEditingTool = null;
    let currentFilter = 'all';

    // Custom Dialog Helpers
    const customDialog = container.querySelector('#custom-dialog-modal');
    const customDialogTitle = container.querySelector('#custom-dialog-title');
    const customDialogMessage = container.querySelector('#custom-dialog-message');
    const customDialogOk = container.querySelector('#custom-dialog-ok');
    const customDialogCancel = container.querySelector('#custom-dialog-cancel');

    const showDialog = (titleText, messageHTML, showCancel = false) => {
        return new Promise(resolve => {
            customDialogTitle.textContent = titleText;
            customDialogMessage.innerHTML = messageHTML;
            customDialog.style.display = 'flex';
            
            if (showCancel) customDialogCancel.style.display = 'block';
            else customDialogCancel.style.display = 'none';

            const onOk = () => { cleanup(); resolve(true); };
            const onCancel = () => { cleanup(); resolve(false); };

            const cleanup = () => {
                customDialog.style.display = 'none';
                customDialogOk.removeEventListener('click', onOk);
                customDialogCancel.removeEventListener('click', onCancel);
            };

            customDialogOk.addEventListener('click', onOk);
            if (showCancel) customDialogCancel.addEventListener('click', onCancel);
        });
    };

    const formatBytes = (bytes) => {
        if (bytes === 0 || !bytes) return '0 B';
        const k = 1024;
        const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
    };

    // Helper to filter and show empty state
    const filterItems = () => {
        const items = listContainer.querySelectorAll('.setting-item');
        let visibleCount = 0;
        items.forEach(item => {
            if (currentFilter === 'all' || item.dataset.type === currentFilter) {
                item.style.display = 'flex';
                visibleCount++;
            } else {
                item.style.display = 'none';
            }
        });
        
        const existingEmpty = listContainer.querySelector('.empty-state-msg');
        if (existingEmpty) existingEmpty.remove();

        if (visibleCount === 0) {
            let hubUrl = currentFilter === 'all' ? 'https://cluaiz.com/hub/' : `https://cluaiz.com/hub/?hubType=${currentFilter}`;
            let msgType = currentFilter === 'all' ? 'tools' : `${currentFilter}s`;
            const emptyEl = document.createElement('div');
            emptyEl.className = 'empty-state-msg';
            emptyEl.style.cssText = 'text-align:center; padding: 60px 20px; color: var(--text-secondary); display: flex; flex-direction: column; align-items: center; gap: 16px; border: 1px dashed var(--border-color); border-radius: 8px;';
            emptyEl.innerHTML = `
                <div>Currently not available. No <b>${msgType}</b> found.</div>
                <a href="${hubUrl}" target="_blank" style="background: var(--accent-color, #3b82f6); color: #fff; text-decoration: none; padding: 10px 20px; border-radius: 8px; font-weight: 500; display: inline-flex; align-items: center; gap: 8px; font-size: 0.9rem; transition: opacity 0.2s;" onmouseover="this.style.opacity='0.9'" onmouseout="this.style.opacity='1'">
                    <i data-lucide="external-link" class="w-4 h-4"></i> Get from Hub
                </a>
            `;
            listContainer.appendChild(emptyEl);
            if (window.lucide) window.lucide.createIcons();
        }
    };

    // Handle Tabs
    tabBtns.forEach(btn => {
        btn.addEventListener('click', () => {
            tabBtns.forEach(b => {
                b.classList.remove('active');
                b.style.background = 'transparent';
                b.style.color = 'var(--text-secondary)';
            });
            btn.classList.add('active');
            btn.style.background = 'var(--accent-color, #3b82f6)';
            btn.style.color = '#fff';
            
            currentFilter = btn.dataset.filter;
            filterItems();
        });
    });

    // Helper: Sync Toggle
    const syncToggle = async (type, name, newState, toggleEl) => {
        try {
            await fetch('/api/components/settings', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    component_type: type,
                    component_id: name,
                    settings: { enabled: newState }
                })
            });
            return true;
        } catch (e) {
            console.error("Failed to save state", e);
            return false;
        }
    };

    const loadTools = async () => {
        listContainer.innerHTML = '<div style="text-align:center; padding: 20px; color: var(--text-secondary);">Loading tools...</div>';
        try {
            const res = await fetch('/api/components/list');
            const data = await res.json();
            
            listContainer.innerHTML = '';

            for (const [type, names] of Object.entries(data)) {
                for (const name of names) {
                    let isEnabled = false;
                    try {
                        const setRes = await fetch(`/api/components/settings?component_type=${type}&component_id=${name}`);
                        const setData = await setRes.json();
                        if (setData.status === 'success' && setData.values && setData.values.enabled) {
                            isEnabled = true;
                        }
                    } catch (e) { console.warn("Could not fetch settings for", name); }

                    const item = document.createElement('div');
                    item.className = 'setting-item';
                    item.style.alignItems = 'center';
                    item.style.cursor = 'pointer';
                    item.dataset.type = type;
                    
                    let tagColor = type === 'skill' ? '#10b981' : (type === 'plugin' ? '#3b82f6' : '#f59e0b');

                    item.innerHTML = `
                        <div class="setting-info" style="pointer-events: none;">
                            <span class="setting-label" style="display:flex; align-items:center; gap:8px;">
                                ${name}
                            </span>
                            <span class="setting-desc" style="color: ${tagColor}; font-size: 0.65rem; margin-top: 4px; text-transform: uppercase; font-weight: bold; display: inline-block;">${type}</span>
                        </div>
                        <div class="setting-control" style="display: flex; gap: 8px;">
                            <div class="setting-toggle card-toggle ${isEnabled ? 'active' : ''}" style="margin-top: 4px;">
                                <div class="setting-toggle-thumb"></div>
                            </div>
                        </div>
                    `;
                    
                    // Card click opens modal
                    item.addEventListener('click', (e) => {
                        if (e.target.closest('.card-toggle')) return; // handled separately
                        openModal(type, name, isEnabled, item.querySelector('.card-toggle'));
                    });

                    // Handle card toggle
                    const toggle = item.querySelector('.card-toggle');
                    toggle.addEventListener('click', async (e) => {
                        e.stopPropagation();
                        const newState = !toggle.classList.contains('active');
                        toggle.classList.toggle('active');
                        const success = await syncToggle(type, name, newState, toggle);
                        if (!success) toggle.classList.toggle('active');
                    });
                    
                    listContainer.appendChild(item);
                }
            }

            filterItems();

        } catch (e) {
            listContainer.innerHTML = `<div style="text-align:center; padding: 20px; color: #ef4444;">Failed to load tools: ${e.message}</div>`;
        }
    };

    const openModal = async (type, name, isEnabled, cardToggleEl) => {
        title.textContent = `${name} (${type})`;
        modal.style.display = 'flex';
        currentEditingTool = { type, id: name, cardToggleEl };
        
        if (isEnabled) modalToggle.classList.add('active');
        else modalToggle.classList.remove('active');

        fileTree.innerHTML = '<div style="color: var(--text-secondary); font-size: 0.8rem;">Loading files...</div>';
        textarea.value = '';
        fileViewerTitle.textContent = 'Select a file to view';

        try {
            const res = await fetch(`/api/components/files?component_type=${type}&component_id=${name}`);
            const data = await res.json();
            if(data.status === 'success') {
                renderFileTree(data.files);
                btnClearTemp.innerHTML = `<i data-lucide="eraser" class="w-4 h-4"></i> Clear Temp Cache (${formatBytes(data.temp_cache_size)})`;
                btnClearAll.innerHTML = `<i data-lucide="trash-2" class="w-4 h-4"></i> Clear All Cache (${formatBytes(data.all_cache_size)})`;
                if(window.lucide) window.lucide.createIcons();
            } else {
                fileTree.innerHTML = `<div style="color:var(--text-secondary); padding:10px;">Failed to load files: ${data.message}</div>`;
                btnClearTemp.innerHTML = `<i data-lucide="eraser" class="w-4 h-4"></i> Clear Temp Cache`;
                btnClearAll.innerHTML = `<i data-lucide="trash-2" class="w-4 h-4"></i> Clear All Cache`;
                if(window.lucide) window.lucide.createIcons();
            }
        } catch(e) {
            fileTree.innerHTML = `<div style="color: #ef4444; font-size: 0.8rem;">Error loading files</div>`;
        }
    };

    const renderFileTree = (files) => {
        fileTree.innerHTML = '';
        files.sort((a,b) => b.is_dir - a.is_dir || a.name.localeCompare(b.name)).forEach(f => {
            const el = document.createElement('div');
            el.style.display = 'flex';
            el.style.alignItems = 'center';
            el.style.gap = '6px';
            el.style.padding = '4px 8px';
            el.style.borderRadius = '4px';
            el.style.cursor = 'pointer';
            el.style.fontSize = '0.85rem';
            el.style.color = 'var(--text-primary)';
            
            const icon = f.is_dir ? 'folder' : 'file-text';
            el.innerHTML = `<i data-lucide="${icon}" class="w-3 h-3 text-secondary"></i> ${f.name}`;
            
            el.addEventListener('mouseover', () => el.style.background = 'rgba(255,255,255,0.05)');
            el.addEventListener('mouseout', () => el.style.background = 'transparent');
            
            if (!f.is_dir) {
                el.addEventListener('click', async () => {
                    fileViewerTitle.textContent = f.path;
                    textarea.value = 'Loading...';
                    try {
                        const res = await fetch(`/api/components/file?component_type=${currentEditingTool.type}&component_id=${currentEditingTool.id}&file_path=${encodeURIComponent(f.path)}`);
                        const data = await res.json();
                        textarea.value = data.status === 'success' ? data.content : `Error: ${data.message}`;
                    } catch(e) { textarea.value = `Error: ${e.message}`; }
                });
            }
            fileTree.appendChild(el);
        });
        if (window.lucide) window.lucide.createIcons();
    };

    // Modal Actions
    modalToggle.addEventListener('click', async () => {
        if(!currentEditingTool) return;
        const newState = !modalToggle.classList.contains('active');
        modalToggle.classList.toggle('active');
        const success = await syncToggle(currentEditingTool.type, currentEditingTool.id, newState, modalToggle);
        if (success && currentEditingTool.cardToggleEl) {
            if (newState) currentEditingTool.cardToggleEl.classList.add('active');
            else currentEditingTool.cardToggleEl.classList.remove('active');
        } else if (!success) {
            modalToggle.classList.toggle('active');
        }
    });

    const clearCache = async (all) => {
        if(!currentEditingTool) return;
        
        const cacheType = all ? 'All Cache' : 'Temp Cache';
        const confirmed = await showDialog('Confirm Action', `Are you sure you want to clear <b>${cacheType}</b> for ${currentEditingTool.id}?`, true);
        if(!confirmed) return;

        try {
            const res = await fetch('/api/components/cache', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    component_type: currentEditingTool.type,
                    component_id: currentEditingTool.id,
                    all: all
                })
            });
            const data = await res.json();
            await showDialog('Notice', data.message || (data.status === 'success' ? 'Cache cleared!' : 'Failed to clear cache'));
            
            if (data.status === 'success') {
                // Refresh files and cache sizes dynamically without closing modal
                const fRes = await fetch(`/api/components/files?component_type=${currentEditingTool.type}&component_id=${currentEditingTool.id}`);
                const fData = await fRes.json();
                if (fData.status === 'success') {
                    renderFileTree(fData.files);
                    btnClearTemp.innerHTML = `<i data-lucide="eraser" class="w-4 h-4"></i> Clear Temp Cache (${formatBytes(fData.temp_cache_size)})`;
                    btnClearAll.innerHTML = `<i data-lucide="trash-2" class="w-4 h-4"></i> Clear All Cache (${formatBytes(fData.all_cache_size)})`;
                    if(window.lucide) window.lucide.createIcons();
                }
            }
        } catch(e) { showDialog('Error', "Error clearing cache: " + e.message); }
    };

    btnClearTemp.addEventListener('click', () => clearCache(false));
    btnClearAll.addEventListener('click', () => clearCache(true));

    btnUninstall.addEventListener('click', async () => {
        if(!currentEditingTool) return;
        const confirmed = await showDialog('Confirm Uninstall', `Are you sure you want to uninstall <b>${currentEditingTool.id}</b>?`, true);
        if(!confirmed) return;
        
        let payloadField = `${currentEditingTool.type}_name`;
        try {
            const res = await fetch(`/v1/${currentEditingTool.type}s/remove`, {
                method: 'DELETE',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ [payloadField]: currentEditingTool.id })
            });
            const data = await res.json();
            if(data.status === 'success') {
                await showDialog('Success', 'Successfully uninstalled');
                modal.style.display = 'none';
                loadTools();
            } else {
                showDialog('Error', data.message);
            }
        } catch(e) { showDialog('Error', "Error uninstalling: " + e.message); }
    });

    closeBtn.addEventListener('click', () => { modal.style.display = 'none'; currentEditingTool = null; });
    
    // Init
    loadTools();
}
