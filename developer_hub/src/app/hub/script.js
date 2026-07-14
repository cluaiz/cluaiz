import { CodeEditor } from '../../../components/editor/editor.js';
import { mountChat } from '../chat/chat_input/chat.js';
import { mountChatStream } from '../chat/chat_stream/chat_stream.js';

let state = {
    editor: null,
    manifestEditor: null,
    isMock: true
};

function injectHubCSS() {
    // Reuse API CSS to match exact theme and layout
    if (!document.getElementById('sidebar-css')) {
        const link = document.createElement('link');
        link.id = 'sidebar-css';
        link.rel = 'stylesheet';
        link.href = '/src/app/api/sidebar/sidebar.css';
        document.head.appendChild(link);
    }
    if (!document.getElementById('editor-css')) {
        const link = document.createElement('link');
        link.id = 'editor-css';
        link.rel = 'stylesheet';
        link.href = '/components/editor/editor.css';
        document.head.appendChild(link);
    }
    if (!document.getElementById('workspace-css')) {
        const link = document.createElement('link');
        link.id = 'workspace-css';
        link.rel = 'stylesheet';
        link.href = '/src/app/api/workspace.css';
        document.head.appendChild(link);
    }
    // Add Hub specific overrides
    if (!document.getElementById('hub-css')) {
        const link = document.createElement('link');
        link.id = 'hub-css';
        link.rel = 'stylesheet';
        link.href = '/src/app/hub/style.css?v=' + new Date().getTime();
        document.head.appendChild(link);
    }
}

export async function mountHubWorkspace(rootElement) {
    injectHubCSS();
    
    try {
        const response = await fetch('/src/app/hub/index.html?v=' + new Date().getTime());
        const html = await response.text();
        rootElement.innerHTML = html;
        
        setupHubLogic();

        // Mount the imported Chat UI directly into the Sidebar's Chat Tester
        const chatContainer = document.getElementById('hub-chat-sidebar-container');
        if (chatContainer) {
            await mountChat(chatContainer);

            // Mount the message history (chat stream)
            const messagesContainer = document.getElementById('chat-messages');
            if (messagesContainer) {
                messagesContainer.style.display = 'flex';
                await mountChatStream(messagesContainer);
            }
        }
    } catch (e) {
        rootElement.innerHTML = `<h2 style="color:red; padding: 20px;">Failed to load Hub Workspace: ${e.message}</h2>`;
    }
}

function setupHubLogic() {
    // Top Tabs
    const tabs = document.querySelectorAll('.tabs .tab[data-target]');
    tabs.forEach(tab => {
        tab.addEventListener('click', (e) => {
            tabs.forEach(t => t.classList.remove('active'));
            document.querySelectorAll('.panel-top .panel-body').forEach(p => p.classList.add('hidden'));
            
            e.target.classList.add('active');
            const targetId = e.target.getAttribute('data-target');
            document.getElementById(targetId).classList.remove('hidden');
            document.getElementById('hub-panel-left-title').textContent = e.target.textContent;
            
            if (state.editor) state.editor.refresh();
            if (state.manifestEditor) state.manifestEditor.refresh();
        });
    });

    // Bottom Tabs
    const bottomTabs = document.querySelectorAll('.tabs .tab[data-bottom]');
    bottomTabs.forEach(tab => {
        tab.addEventListener('click', (e) => {
            bottomTabs.forEach(t => t.classList.remove('active'));
            document.querySelectorAll('#hub-res-body-container > div').forEach(p => p.style.display = 'none');
            
            e.target.classList.add('active');
            const targetId = e.target.getAttribute('data-bottom');
            document.getElementById(targetId).style.display = 'block';
        });
    });

    // Sidebar Accordions
    const groupTitles = document.querySelectorAll('.nav-group-title');
    groupTitles.forEach(title => {
        title.addEventListener('click', (e) => {
            const targetId = e.currentTarget.getAttribute('data-group');
            const targetGroup = document.getElementById(targetId);
            if (targetGroup.classList.contains('open')) {
                targetGroup.classList.remove('open');
                e.currentTarget.classList.remove('open');
            } else {
                targetGroup.classList.add('open');
                e.currentTarget.classList.add('open');
            }
        });
    });

    // Sidebar Toggle
    const sidebar = document.getElementById('hub-sidebar');
    const closeSidebarBtn = document.getElementById('hub-sidebar-close-btn');
    const toggleSidebarBtn = document.getElementById('hub-sidebar-open-btn');
    const resizer = document.getElementById('hub-sidebar-resizer');

    closeSidebarBtn.addEventListener('click', () => {
        sidebar.style.display = 'none';
        resizer.style.display = 'none';
        toggleSidebarBtn.classList.remove('hidden');
    });

    toggleSidebarBtn.addEventListener('click', () => {
        sidebar.style.display = 'flex';
        resizer.style.display = 'block';
        toggleSidebarBtn.classList.add('hidden');
    });

    // Mock vs Real Engine Toggle
    const mockBtn = document.querySelector('.mock-btn');
    const realBtn = document.querySelector('.real-btn');

    mockBtn.addEventListener('click', () => {
        mockBtn.classList.add('active');
        mockBtn.style.backgroundColor = 'var(--text-accent)';
        mockBtn.style.color = '#000';
        realBtn.classList.remove('active');
        realBtn.style.backgroundColor = 'transparent';
        realBtn.style.color = 'var(--text-muted)';
        state.isMock = true;
        addLogEntry('Switched to Fake AI (Mock) Engine');
    });

    realBtn.addEventListener('click', () => {
        realBtn.classList.add('active');
        realBtn.style.backgroundColor = 'var(--text-accent)';
        realBtn.style.color = '#000';
        mockBtn.classList.remove('active');
        mockBtn.style.backgroundColor = 'transparent';
        mockBtn.style.color = 'var(--text-muted)';
        state.isMock = false;
        addLogEntry('Switched to Real Cluaiz Engine');
    });

    // Initialize CodeMirror via API Toolkit's CodeEditor class (Now just a Snippet Tester)
    const snippetContainer = document.getElementById('hub-snippet-container');
    if (snippetContainer) {
        state.editor = new CodeEditor({
            id: 'hub-snippet-editor',
            mode: 'rust',
            value: `// Snippet Tester\n// Test 1-2 line scripts here\n\nprintln!("Hello from test snippet");`
        });
        snippetContainer.appendChild(state.editor.render());
        state.editor.mount();
    }

    const manifestContainer = document.getElementById('hub-manifest-container');
    if (manifestContainer) {
        const defaultManifest = {
            "id": "my-plugin",
            "name": "My Plugin",
            "version": "1.0.0",
            "description": "Test plugin for Cluaiz",
            "discovery": {
                "semantic_triggers": ["run test"]
            },
            "permissions": {
                "level": "strict",
                "network_access": false
            }
        };
        state.manifestEditor = new CodeEditor({
            id: 'hub-manifest-editor',
            mode: 'json',
            value: JSON.stringify(defaultManifest, null, 4)
        });
        manifestContainer.appendChild(state.manifestEditor.render());
        state.manifestEditor.mount();
    }

    // Run Test & Publish Logic -> Changed to "Load Unpacked (Install)"
    const installBtn = document.getElementById('hub-btn-install');
    if (installBtn) {
        installBtn.addEventListener('click', async () => {
            addLogEntry('Initiating Local Installation...');
            const consoleTab = document.getElementById('tab-console');
            consoleTab.innerHTML = '<div style="color: var(--text-muted);">Copying binary and manifest to local registry...</div>';
            
            // Ensure response panel is open
            const bodyContainer = document.getElementById('hub-res-body-container');
            if (bodyContainer.classList.contains('hidden')) {
                document.getElementById('hub-response-header').click();
            }

            try {
                const manifestStr = state.manifestEditor.getValue();
                const manifest = JSON.parse(manifestStr);
                addLogEntry(`Validating manifest for ${manifest.id}...`);

                if (state.isMock) {
                    setTimeout(() => {
                        addLogEntry('Mock Engine: Installed locally at ~/.cluaiz/plugins/');
                        addLogEntry('Sandbox initialized. Ready for Chat Trigger Testing.');
                        consoleTab.innerHTML += '<div style="color: var(--method-get); margin-top: 10px;">SUCCESS: Installed locally. Please switch to Chat Tester tab and type a message to trigger it.</div>';
                    }, 800);
                } else {
                    addLogEntry('Sending to Real Cluaiz Engine Registry API...');
                    setTimeout(() => {
                        addLogEntry('Cluaiz Engine returned 501 Not Implemented (Endpoint missing).');
                        consoleTab.innerHTML += '<div style="color: var(--method-post); margin-top: 10px;">ERROR: Real Engine installation failed.</div>';
                    }, 1000);
                }
            } catch (e) {
                addLogEntry('Installation Aborted: Manifest parsing failed - ' + e.message);
                consoleTab.innerHTML += `<div style="color: var(--method-del); margin-top: 10px;">JSON Syntax Error: ${e.message}</div>`;
            }
        });
    }

    // Response Panel Toggle
    document.getElementById('hub-response-header').addEventListener('click', () => {
        const topPanel = document.getElementById('hub-panel-top-container');
        const bottomPanel = document.getElementById('hub-panel-bottom-container');
        const bodyContainer = document.getElementById('hub-res-body-container');
        const tabsContainer = document.querySelector('#hub-panel-bottom-container .tabs');
        const icon = document.getElementById('hub-response-toggle-icon');
        const isHidden = bodyContainer.classList.contains('hidden');

        if (isHidden) {
            bodyContainer.classList.remove('hidden');
            tabsContainer.classList.remove('hidden');
            icon.textContent = "▼ Logs & Output";
            bottomPanel.style.flex = "1";
            if (topPanel.dataset.lastHeight) {
                topPanel.style.height = topPanel.dataset.lastHeight;
            } else {
                topPanel.style.height = "50%";
            }
            topPanel.style.flex = "";
        } else {
            bodyContainer.classList.add('hidden');
            tabsContainer.classList.add('hidden');
            icon.textContent = "▶ Logs & Output";
            bottomPanel.style.flex = "0 0 44px"; 
            topPanel.dataset.lastHeight = topPanel.style.height;
            topPanel.style.height = "auto";
            topPanel.style.flex = "1";
        }
        setTimeout(() => { 
            if (state.editor) state.editor.refresh(); 
            if (state.manifestEditor) state.manifestEditor.refresh();
        }, 50);
    });

    initResizers();
}

function initResizers() {
    // Vertical Resizer (Top/Bottom panels)
    const vertResizer = document.getElementById('hub-drag-resizer');
    const topPanel = document.getElementById('hub-panel-top-container');
    let isDraggingVert = false;

    vertResizer.addEventListener('mousedown', function (e) {
        isDraggingVert = true;
        document.body.style.cursor = 'ns-resize';
        vertResizer.classList.add('dragging');
    });

    document.addEventListener('mousemove', function (e) {
        if (!isDraggingVert) return;
        const panelsContainer = document.querySelector('.panels');
        if (!panelsContainer) return;
        const containerOffset = panelsContainer.getBoundingClientRect().top;
        const pointerRelativeYpos = e.clientY - containerOffset;
        const containerHeight = panelsContainer.getBoundingClientRect().height;
        if (pointerRelativeYpos > 100 && pointerRelativeYpos < containerHeight - 40) {
            const newHeight = (pointerRelativeYpos / containerHeight) * 100;
            topPanel.style.height = `${newHeight}%`;
        }
    });

    document.addEventListener('mouseup', function (e) {
        if (!isDraggingVert) return;
        isDraggingVert = false;
        document.body.style.cursor = 'default';
        vertResizer.classList.remove('dragging');
        if (state.editor) state.editor.refresh();
        if (state.manifestEditor) state.manifestEditor.refresh();
    });

    // Horizontal Resizer (Sidebar)
    const horizResizer = document.getElementById('hub-sidebar-resizer');
    const sidebar = document.getElementById('hub-sidebar');
    let isDraggingHoriz = false;

    // Load saved width
    const savedWidth = localStorage.getItem('hub-sidebar-width');
    if (savedWidth) {
        sidebar.style.width = savedWidth;
    }

    horizResizer.addEventListener('mousedown', function (e) {
        isDraggingHoriz = true;
        document.body.style.cursor = 'ew-resize';
        horizResizer.classList.add('dragging');
    });

    document.addEventListener('mousemove', function (e) {
        if (!isDraggingHoriz) return;
        let newWidth = e.clientX;
        if (newWidth < 340) newWidth = 340; // match CSS min-width
        if (newWidth > 800) newWidth = 800;
        sidebar.style.width = `${newWidth}px`;
    });

    document.addEventListener('mouseup', function (e) {
        if (!isDraggingHoriz) return;
        isDraggingHoriz = false;
        document.body.style.cursor = 'default';
        horizResizer.classList.remove('dragging');
        localStorage.setItem('hub-sidebar-width', sidebar.style.width);
        if (state.editor) state.editor.refresh();
        if (state.manifestEditor) state.manifestEditor.refresh();
    });
}

function addLogEntry(message) {
    const timeline = document.getElementById('tab-timeline');
    if (!timeline) return;
    const entry = document.createElement('div');
    entry.style.marginBottom = '4px';
    const now = new Date();
    const timeStr = `${now.getHours().toString().padStart(2,'0')}:${now.getMinutes().toString().padStart(2,'0')}:${now.getSeconds().toString().padStart(2,'0')}`;
    entry.innerHTML = `<span style="color: var(--text-accent);">[${timeStr}]</span> ${message}`;
    timeline.appendChild(entry);
    timeline.scrollTop = timeline.scrollHeight;
}
