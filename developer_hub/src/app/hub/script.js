export async function mountHubWorkspace(rootElement) {
    try {
        const response = await fetch('/src/app/hub/index.html?v=' + new Date().getTime());
        const html = await response.text();
        rootElement.innerHTML = html;
        
        // Add CSS if not already present
        if (!document.getElementById('hub-css')) {
            const link = document.createElement('link');
            link.id = 'hub-css';
            link.rel = 'stylesheet';
            link.href = '/src/app/hub/style.css?v=' + new Date().getTime();
            document.head.appendChild(link);
        }

        // Initialize Lucide icons if available
        if (window.lucide) {
            window.lucide.createIcons();
        }

        setupHubLogic();
    } catch (e) {
        rootElement.innerHTML = `<h2 style="color:red; padding: 20px;">Failed to load Hub Workspace: ${e.message}</h2>`;
    }
}

function setupHubLogic() {
    // Tab switching for Sidebar
    const sidebarTabs = document.querySelectorAll('.hub-sidebar .sidebar-tabs .tab-btn');
    sidebarTabs.forEach(tab => {
        tab.addEventListener('click', (e) => {
            // Remove active from all tabs
            sidebarTabs.forEach(t => t.classList.remove('active'));
            document.querySelectorAll('.hub-sidebar .tab-pane').forEach(p => p.classList.remove('active'));
            
            // Add active to clicked tab
            e.target.classList.add('active');
            const targetId = e.target.getAttribute('data-target');
            document.getElementById(targetId).classList.add('active');
        });
    });

    // Tab switching for Bottom Panel
    const panelTabs = document.querySelectorAll('.bottom-panel .panel-tabs .tab-btn');
    panelTabs.forEach(tab => {
        tab.addEventListener('click', (e) => {
            panelTabs.forEach(t => t.classList.remove('active'));
            document.querySelectorAll('.bottom-panel .tab-pane').forEach(p => p.classList.remove('active'));
            
            e.target.classList.add('active');
            const targetId = e.target.getAttribute('data-target');
            document.getElementById(targetId).classList.add('active');
        });
    });

    // Sidebar Toggle
    const sidebar = document.getElementById('hub-sidebar');
    const closeSidebarBtn = document.getElementById('close-sidebar-btn');
    const toggleSidebarBtn = document.getElementById('toggle-sidebar-btn');

    closeSidebarBtn.addEventListener('click', () => {
        sidebar.classList.add('collapsed');
        toggleSidebarBtn.style.display = 'block';
    });

    toggleSidebarBtn.addEventListener('click', () => {
        sidebar.classList.remove('collapsed');
        toggleSidebarBtn.style.display = 'none';
    });

    // Mock vs Real Engine Toggle
    const mockBtn = document.querySelector('.mock-btn');
    const realBtn = document.querySelector('.real-btn');

    mockBtn.addEventListener('click', () => {
        mockBtn.classList.add('active');
        realBtn.classList.remove('active');
        addLogEntry('Switched to Fake AI (Mock) Engine');
    });

    realBtn.addEventListener('click', () => {
        realBtn.classList.add('active');
        mockBtn.classList.remove('active');
        addLogEntry('Switched to Real Cluaiz Engine');
    });

    // Initialize CodeMirror Editor
    const editorMount = document.getElementById('hub-editor-mount');
    if (window.CodeMirror) {
        const editor = window.CodeMirror(editorMount, {
            value: `// Cluaiz Plugin Lab\n// Test your MCP, Plugins, or Skills here.\n\nfunction handleRequest(req) {\n    console.log("Mock request received", req);\n    return { status: "success" };\n}`,
            mode: "javascript",
            theme: "darcula",
            lineNumbers: true,
            autoCloseBrackets: true,
            matchBrackets: true,
            indentUnit: 4,
            tabSize: 4
        });
    } else {
        editorMount.innerHTML = '<div style="padding:20px;">CodeMirror not loaded.</div>';
    }
}

function addLogEntry(message) {
    const timeline = document.getElementById('timeline');
    const entry = document.createElement('div');
    entry.className = 'log-entry info';
    const now = new Date();
    const timeStr = `${now.getHours().toString().padStart(2,'0')}:${now.getMinutes().toString().padStart(2,'0')}:${now.getSeconds().toString().padStart(2,'0')}`;
    entry.innerHTML = `<span class="time">[${timeStr}]</span> ${message}`;
    timeline.appendChild(entry);
    timeline.scrollTop = timeline.scrollHeight;
}
