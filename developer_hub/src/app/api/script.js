import { Sidebar } from './sidebar/sidebar.js';
import { CodeEditor } from '../../../components/editor/editor.js';
import { Dropdown } from '../../../components/dropdown/dropdown.js';

const state = {
    apiData: [],
    activeEndpoint: null,
    editor: null,
    headersEditor: null,
    methodDropdown: null,
    protocolDropdown: null,
    languageDropdown: null,
    sidebarComponent: null
};

function injectApiCSS() {
    if (!document.getElementById('sidebar-css')) {
        const link = document.createElement('link');
        link.id = 'sidebar-css';
        link.rel = 'stylesheet';
        link.href = './src/app/api/sidebar/sidebar.css';
        document.head.appendChild(link);
    }
    if (!document.getElementById('dropdown-css')) {
        const link = document.createElement('link');
        link.id = 'dropdown-css';
        link.rel = 'stylesheet';
        link.href = '/components/dropdown/dropdown.css';
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
}

export async function mountApiWorkspace(rootElement) {
    injectApiCSS();
    
    try {
        const res = await fetch('/src/app/api/index.html');
        if (res.ok) {
            const html = await res.text();
            // Remove the inline script tag from the fetched HTML if any
            const cleanedHtml = html.replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '');
            rootElement.innerHTML = cleanedHtml;

            // Make toggleSidebar globally available
            window.app = window.app || {};
            window.app.toggleSidebar = function () {
                const sidebar = document.getElementById('sidebar');
                const resizer = document.getElementById('sidebar-resizer');
                const openBtn = document.getElementById('sidebar-open-btn-api');
                
                if (sidebar && sidebar.style.display !== 'none') {
                    sidebar.style.display = 'none';
                    if (resizer) resizer.style.display = 'none';
                    if (openBtn) openBtn.classList.remove('hidden');
                } else if (sidebar) {
                    sidebar.style.display = 'flex';
                    if (resizer) resizer.style.display = 'block';
                    if (openBtn) openBtn.classList.add('hidden');
                }
            };
            
            initApp();
        } else {
            rootElement.innerHTML = '<div style="color: white; padding: 20px;">Failed to load API Workspace HTML.</div>';
        }
    } catch (e) {
        console.error("Failed to mount API Workspace:", e);
    }
}

async function initApp() {
    console.log("initApp called");
    
    // 1. Fetch API Data & Render Sidebar
    try {
        const endpoints = [
            'system.json',
            'inference.json',
            'models.json',
            'plugins.json',
            'skills.json',
            'mcp.json',
            'extensions.json',
            'config.json',
            'tuning.json'
        ];
        
        const responses = await Promise.all(
            endpoints.map(file => fetch(`./data/${file}`).then(res => res.json()).catch(err => {
                console.error(`Failed to load ${file}:`, err);
                return null;
            }))
        );
        
        // Filter out any failed loads
        state.apiData = responses.filter(data => data !== null);

        if (state.apiData.length > 0 && state.apiData[0].endpoints.length > 0) {
            openEndpoint(state.apiData[0].endpoints[0]);
        }
    } catch (e) {
        console.error("Failed to render initial view:", e);
    }
    
    mountApiListeners();

    // 4. Initialize Other UI Components
    try {
        setupCustomSelects();
        initEditor();
        initResizer();
        initSidebarResizer();
    } catch (e) {
        console.error("Failed to initialize UI components:", e);
    }

    // 5. Render Sidebar (Must be after editors are initialized!)
    renderSidebar();
}

function mountApiListeners() {
    document.getElementById('tab-params').addEventListener('click', () => switchTab('params'));
    document.getElementById('tab-headers').addEventListener('click', () => switchTab('headers'));
    document.getElementById('tab-docs').addEventListener('click', () => switchTab('docs'));
    document.getElementById('tab-snippets').addEventListener('click', () => switchTab('snippets'));

    document.getElementById('response-header').addEventListener('click', () => toggleResponsePanel());
    document.getElementById('btn-send').addEventListener('click', sendRequest);
}

function renderSidebar() {
    const navContainer = document.getElementById('nav-container');
    if (!navContainer) return;
    navContainer.innerHTML = '';
    
    state.sidebarComponent = new Sidebar({
        apiData: state.apiData,
        onSelect: (ep) => {
            openEndpoint(ep);
        }
    });
    
    navContainer.appendChild(state.sidebarComponent.render());
}

function openEndpoint(ep) {
    state.activeEndpoint = ep;

    // Update Custom Select Value for Method
    updateCustomSelect('custom-req-method', ep.method);

    // Calculate Available Methods for this specific Path
    let currentGroup = state.apiData.find(g => g.endpoints.includes(ep));
    if (currentGroup) {
        const endpointsWithSamePath = currentGroup.endpoints.filter(e => e.path === ep.path);
        const availableMethods = [...new Set(endpointsWithSamePath.map(e => e.method))];
        updateAvailableMethods('custom-req-method', availableMethods);
    }

    document.getElementById('req-url').value = `${window.location.protocol}//${window.location.host}${ep.path}`;

    // Detect if this is a raw code endpoint (CEL or FFI)
    let isRawCode = false;
    let targetProtocol = 'http';
    if (ep.path.includes('/cel/execute')) {
        isRawCode = true;
        targetProtocol = 'cel';
    } else if (ep.path.includes('/execute/')) {
        isRawCode = true;
        targetProtocol = 'c-pointer';
    }

    updateCustomSelect('custom-req-protocol', targetProtocol);
    onProtocolChange(false); // Trigger UI update without clearing default payload

    // Generate default payload
    const bodyEditor = document.getElementById('req-body');
    const payloadDesc = document.getElementById('payload-desc');

    if (ep.method === 'POST' || ep.method === 'PUT' || ep.method === 'DELETE') {
        if (isRawCode) {
            // Put raw code in editor, not wrapped in JSON
            let rawCode = "";
            if (ep.params && ep.params.length > 0 && ep.params[0].default !== undefined) {
                rawCode = ep.params[0].default;
            }
            if (state.editor) state.editor.setValue(rawCode);
            if (state.editor) state.editor.setOption("readOnly", false);
        } else {
            // Build JSON object
            let obj = {};
            if (ep.params && ep.params.length > 0) {
                ep.params.forEach(p => {
                    if (p.default !== undefined) {
                        obj[p.name] = p.default;
                    } else if (p.type === 'string') {
                        obj[p.name] = "value";
                    } else if (p.type === 'integer' || p.type === 'float') {
                        obj[p.name] = 0;
                    } else if (p.type === 'array') {
                        obj[p.name] = [];
                    } else if (p.type === 'boolean') {
                        obj[p.name] = false;
                    } else {
                        obj[p.name] = null;
                    }
                });
                if (state.editor) state.editor.setValue(JSON.stringify(obj, null, 2));
                if (state.editor) {
                    state.editor.setOption("mode", "application/json");
                    state.editor.setOption("readOnly", false);
                }
            } else {
                if (state.editor) state.editor.setValue("{}");
                if (state.editor) {
                    state.editor.setOption("mode", "application/json");
                    state.editor.setOption("readOnly", false);
                }
            }
        }
    } else {
        if (state.editor) {
            state.editor.setOption("mode", "text/plain");
            state.editor.setValue("");
            state.editor.setOption("readOnly", "nocursor");
        }
    }

    // Load documentation asynchronously if available
    const docsContent = document.getElementById('docs-content');
    if (ep.docs_url) {
        docsContent.innerHTML = `<em>Loading documentation from CDN...</em><br/><br/><span style="color:#8b949e; font-size: 12px;">${ep.docs_url}</span>`;
        fetch(ep.docs_url)
            .then(r => r.text())
            .then(text => {
                if (typeof marked !== 'undefined') {
                    docsContent.innerHTML = `<div class="markdown-body">${marked.parse(text)}</div>`;
                } else {
                    const escaped = text.replace(/</g, "&lt;").replace(/>/g, "&gt;");
                    docsContent.innerHTML = `<pre style="white-space: pre-wrap; font-family: monospace; color: #c9d1d9;">${escaped}</pre>`;
                }
            })
            .catch(err => {
                docsContent.innerHTML = `<span class="status-err">Failed to fetch documentation from CDN.</span>`;
            });
    } else {
        docsContent.innerHTML = "<em>No specific documentation provided for this endpoint.</em>";
    }

    switchTab('params');

    // Render Snippets
    const snippetsTabs = document.getElementById('snippets-tabs');
    const snippetsContent = document.getElementById('snippets-content');
    if (snippetsTabs && snippetsContent) {
        snippetsTabs.innerHTML = '';
        
        if (ep.examples && ep.examples.length > 0) {
            let first = true;
            ep.examples.forEach((ex) => {
                const btn = document.createElement('button');
                btn.className = 'snippet-tab-btn' + (first ? ' active' : '');
                btn.style.cssText = 'background: transparent; border: none; color: ' + (first ? '#fff' : '#8b949e') + '; cursor: pointer; font-size: 13px; font-weight: 500; padding: 4px 8px; border-bottom: 2px solid ' + (first ? '#2f81f7' : 'transparent') + ';';
                btn.textContent = ex.title;
                
                btn.onclick = () => {
                    Array.from(snippetsTabs.children).forEach(c => {
                        c.style.color = '#8b949e';
                        c.style.borderBottom = '2px solid transparent';
                    });
                    btn.style.color = '#fff';
                    btn.style.borderBottom = '2px solid #2f81f7';
                    
                    const escapedCode = ex.code.replace(/</g, "&lt;").replace(/>/g, "&gt;");
                    snippetsContent.innerHTML = `<pre style="white-space: pre-wrap; font-family: monospace; color: #c9d1d9; background: #0d1117; padding: 12px; border-radius: 6px; border: 1px solid var(--border); overflow-x: auto; margin: 0;">${escapedCode}</pre>`;
                };
                snippetsTabs.appendChild(btn);
                
                if (first) {
                    const escapedCode = ex.code.replace(/</g, "&lt;").replace(/>/g, "&gt;");
                    snippetsContent.innerHTML = `<pre style="white-space: pre-wrap; font-family: monospace; color: #c9d1d9; background: #0d1117; padding: 12px; border-radius: 6px; border: 1px solid var(--border); overflow-x: auto; margin: 0;">${escapedCode}</pre>`;
                    first = false;
                }
            });
        } else {
            snippetsContent.innerHTML = "<em>No snippets available for this endpoint.</em>";
        }
    }

    // Reset response
    document.getElementById('res-body').textContent = "Hit \"Send\" to execute the request.";
    document.getElementById('res-body').style.color = "#8b949e";
    document.getElementById('res-status').innerHTML = "<span>Status: -</span><span>Time: - ms</span><span>Size: - B</span>";
}

export async function initApiTester() {
    renderHome();
    switchTab('params');
    setTimeout(() => {
        if (state.editor) state.editor.refresh();
        if (state.headersEditor) state.headersEditor.refresh();
    }, 100);
}

    export function switchTab(tab) {
        const tabParams = document.getElementById('tab-params');
        const tabHeaders = document.getElementById('tab-headers');
        const tabDocs = document.getElementById('tab-docs');
        const tabSnippets = document.getElementById('tab-snippets');
        const panelParams = document.getElementById('panel-params');
        const panelHeaders = document.getElementById('panel-headers');
        const panelDocs = document.getElementById('panel-docs');
        const panelSnippets = document.getElementById('panel-snippets');
        const panelTitle = document.getElementById('panel-left-title');

        [tabParams, tabHeaders, tabDocs, tabSnippets].forEach(t => t && t.classList.remove('active'));
        [panelParams, panelHeaders, panelDocs, panelSnippets].forEach(p => p && p.classList.add('hidden'));

        if (tab === 'params') {
            if (tabParams) tabParams.classList.add('active');
            if (panelParams) panelParams.classList.remove('hidden');
            if (panelTitle) panelTitle.textContent = "Request Payload";
            if (state.editor) setTimeout(() => state.editor.refresh(), 10);
        } else if (tab === 'headers') {
            if (tabHeaders) tabHeaders.classList.add('active');
            if (panelHeaders) panelHeaders.classList.remove('hidden');
            if (panelTitle) panelTitle.textContent = "Request Headers";
            if (state.headersEditor) setTimeout(() => state.headersEditor.refresh(), 10);
        } else if (tab === 'docs') {
            if (tabDocs) tabDocs.classList.add('active');
            if (panelDocs) panelDocs.classList.remove('hidden');
            if (panelTitle) panelTitle.textContent = "Documentation";
        } else if (tab === 'snippets') {
            if (tabSnippets) tabSnippets.classList.add('active');
            if (panelSnippets) panelSnippets.classList.remove('hidden');
            if (panelTitle) panelTitle.textContent = "Code Snippets";
        }
    }



    export function initEditor() {
        const editorContainer = document.getElementById("editor-container");
        if (editorContainer) {
            state.editor = new CodeEditor({
                id: 'req-body-editor',
                mode: 'application/json',
                value: ''
            });
            editorContainer.appendChild(state.editor.render());
            state.editor.mount();
        }

        const headersContainer = document.getElementById("headers-editor-container");
        if (headersContainer) {
            const savedHeaders = localStorage.getItem('apiTesterHeaders');
            const initialHeaders = savedHeaders ? savedHeaders : '{\n  "Authorization": "Bearer ..."\n}';
            
            state.headersEditor = new CodeEditor({
                id: 'headers-body-editor',
                mode: 'application/json',
                value: initialHeaders,
                onChange: (val) => {
                    localStorage.setItem('apiTesterHeaders', val);
                }
            });
            headersContainer.appendChild(state.headersEditor.render());
            state.headersEditor.mount();
        }
    }

    export function initResizer() {
        const resizer = document.getElementById('drag-resizer');
        const topPanel = document.getElementById('panel-top-container');
        let isDragging = false;

        resizer.addEventListener('mousedown', function (e) {
            isDragging = true;
            document.body.style.cursor = 'ns-resize';
            resizer.classList.add('dragging');
        });

        document.addEventListener('mousemove', function (e) {
            if (!isDragging) return;
            const containerOffset = document.querySelector('.panels').getBoundingClientRect().top;
            const pointerRelativeYpos = e.clientY - containerOffset;
            const containerHeight = document.querySelector('.panels').getBoundingClientRect().height;
            // Min height for top is 100px, min for bottom is 40px
            if (pointerRelativeYpos > 100 && pointerRelativeYpos < containerHeight - 40) {
                const newHeight = (pointerRelativeYpos / containerHeight) * 100;
                topPanel.style.height = `${newHeight}%`;
            }
        });

        document.addEventListener('mouseup', function (e) {
            if (!isDragging) return;
            isDragging = false;
            document.body.style.cursor = 'default';
            resizer.classList.remove('dragging');
            if (state.editor) state.editor.refresh();
        });
    }

    export function initSidebarResizer() {
        const resizer = document.getElementById('sidebar-resizer');
        const sidebar = document.getElementById('sidebar');
        let isDraggingSidebar = false;

        resizer.addEventListener('mousedown', function (e) {
            isDraggingSidebar = true;
            document.body.style.cursor = 'ew-resize';
            resizer.classList.add('dragging');
        });

        document.addEventListener('mousemove', function (e) {
            if (!isDraggingSidebar) return;
            let newWidth = e.clientX;
            // set min and max width constraints
            if (newWidth < 150) newWidth = 150;
            if (newWidth > 600) newWidth = 600;
            sidebar.style.width = `${newWidth}px`;
        });

        document.addEventListener('mouseup', function (e) {
            if (!isDraggingSidebar) return;
            isDraggingSidebar = false;
            document.body.style.cursor = 'default';
            resizer.classList.remove('dragging');
            if (state.editor) state.editor.refresh();
        });
    }

    export function toggleSidebar() {
        const sidebar = document.getElementById('sidebar');
        const resizer = document.getElementById('sidebar-resizer');
        const openBtnApi = document.getElementById('sidebar-open-btn-api');
        const openBtnDashboard = document.getElementById('sidebar-open-btn-dashboard');
        const isHidden = sidebar.classList.contains('hidden');

        if (isHidden) {
            sidebar.classList.remove('hidden');
            resizer.classList.remove('hidden');
            if (openBtnApi) openBtnApi.classList.add('hidden');
            if (openBtnDashboard) openBtnDashboard.classList.add('hidden');
        } else {
            sidebar.classList.add('hidden');
            resizer.classList.add('hidden');
            if (openBtnApi) openBtnApi.classList.remove('hidden');
            if (openBtnDashboard) openBtnDashboard.classList.remove('hidden');
        }

        setTimeout(() => {
            if (state.editor) state.editor.refresh();
        }, 50);
    }

    export function toggleResponsePanel(forceOpen = false) {
        const topPanel = document.getElementById('panel-top-container');
        const bottomPanel = document.getElementById('panel-bottom-container');
        const bodyContainer = document.getElementById('res-body-container');
        const icon = document.getElementById('response-toggle-icon');
        const isHidden = bodyContainer.classList.contains('hidden');

        if (forceOpen && !isHidden) return; // already open

        if (isHidden || forceOpen) {
            bodyContainer.classList.remove('hidden');
            icon.textContent = "▼ Response";
            bottomPanel.style.flex = "1";
            // Restore previous height or remove flex to let height rule again
            if (topPanel.dataset.lastHeight) {
                topPanel.style.height = topPanel.dataset.lastHeight;
            } else {
                topPanel.style.height = "50%";
            }
            topPanel.style.flex = "";
        } else {
            bodyContainer.classList.add('hidden');
            icon.textContent = "▶ Response";
            bottomPanel.style.flex = "0 0 44px"; // Collapse to header size
            // Save height to restore later, and let it take all remaining space
            topPanel.dataset.lastHeight = topPanel.style.height;
            topPanel.style.height = "auto";
            topPanel.style.flex = "1";
        }

        // Refresh editor layout when panels resize
        setTimeout(() => {
            if (state.editor) state.editor.refresh();
        }, 50);
    }

    export function setupCustomSelects() {
        const methodContainer = document.getElementById('method-container');
        if (methodContainer) {
            state.methodDropdown = new Dropdown({
                id: 'custom-req-method',
                options: ['GET', 'POST', 'PUT', 'DELETE'],
                defaultValue: 'GET',
                onChange: (val) => onMethodChange(val)
            });
            methodContainer.appendChild(state.methodDropdown.render());
        }

        const protocolContainer = document.getElementById('protocol-container');
        if (protocolContainer) {
            state.protocolDropdown = new Dropdown({
                id: 'custom-req-protocol',
                options: [
                    {value: 'http', label: 'HTTP / REST'},
                    {value: 'c-pointer', label: 'C-Pointer (FFI)'}
                ],
                defaultValue: 'http',
                onChange: (val) => onProtocolChange()
            });
            protocolContainer.appendChild(state.protocolDropdown.render());
        }

        const languageContainer = document.getElementById('language-container');
        if (languageContainer) {
            state.languageDropdown = new Dropdown({
                id: 'custom-req-language',
                options: [
                    {value: 'rust', label: 'Rust'},
                    {value: 'c', label: 'C / C++'},
                    {value: 'python', label: 'Python (ctypes)'},
                    {value: 'js', label: 'Node.js (FFI)'}
                ],
                defaultValue: 'rust',
                onChange: (val) => onLanguageChange(true)
            });
            languageContainer.appendChild(state.languageDropdown.render());
            languageContainer.classList.add('hidden'); // hidden by default for HTTP
        }
    }

    export function closeAllSelect(elmnt) {
        // Obsolete: Handled internally by Dropdown component
    }

    export function updateCustomSelect(id, val) {
        if (id === 'custom-req-method' && state.methodDropdown) {
            state.methodDropdown.setValue(val);
        } else if (id === 'custom-req-protocol' && state.protocolDropdown) {
            state.protocolDropdown.setValue(val);
        } else if (id === 'custom-req-language' && state.languageDropdown) {
            state.languageDropdown.setValue(val);
        }
    }

    export function updateAvailableMethods(id, availableMethods) {
        if (id === 'custom-req-method' && state.methodDropdown) {
            const allMethods = ['GET', 'POST', 'PUT', 'DELETE'];
            const options = allMethods.map(m => ({
                value: m,
                label: m,
                disabled: !availableMethods.includes(m)
            }));
            state.methodDropdown.updateOptions(options, true);
        }
    }

    export function onMethodChange(newMethod) {
        if (!state.activeEndpoint) return;
        const currentGroup = state.apiData.find(g => g.endpoints.includes(state.activeEndpoint));
        if (!currentGroup) return;

        // Find the endpoint with the SAME path that matches the new method
        const targetEp = currentGroup.endpoints.find(e => e.path === state.activeEndpoint.path && e.method === newMethod);
        if (targetEp) {
            // Update sidebar selection visually via component
            if (state.sidebarComponent) {
                state.sidebarComponent.selectEndpoint(targetEp.path, targetEp.method);
            }
            openEndpoint(targetEp);
        }
    }

    export function onProtocolChange(resetPayload = true) {
        const protocol = state.protocolDropdown ? state.protocolDropdown.getValue() : 'http';
        const methodSelect = document.getElementById('method-container');
        const langSelect = document.getElementById('language-container');
        const urlInput = document.getElementById('req-url');

        // Reset visibility
        if (methodSelect) methodSelect.classList.remove('hidden');
        if (langSelect) langSelect.classList.add('hidden');
        if (state.editor) state.editor.setOption("readOnly", false);

        if (protocol === 'http') {
            if (langSelect) langSelect.classList.remove('hidden');
            if (methodSelect) methodSelect.classList.remove('hidden');
            
            if (state.languageDropdown) {
                state.languageDropdown.updateOptions([
                    {value: 'json', label: 'JSON'},
                    {value: 'cel', label: 'CEL (cluaiz Engine Language)'},
                    {value: 'rhai', label: 'Rhai Script'},
                    {value: 'wasm', label: 'WASM (Rust)'},
                    {value: 'js', label: 'JavaScript (V8)'}
                ], false);
            }

            if (state.editor) {
                state.editor.setOption("mode", "application/json");
                if (resetPayload) state.editor.setValue('{\n  \n}');
            }
            if (resetPayload) {
                updateCustomSelect('custom-req-language', 'json');
                urlInput.value = `${window.location.protocol}//${window.location.host}/health`;
                urlInput.placeholder = `${window.location.protocol}//${window.location.host}/api/...`;
                updateAvailableMethods('custom-req-method', ['GET', 'POST', 'PUT', 'DELETE']);
                updateCustomSelect('custom-req-method', 'GET');
            }
        } else if (protocol === 'c-pointer') {
            if (methodSelect) methodSelect.classList.add('hidden');
            if (langSelect) langSelect.classList.remove('hidden');
            
            // Rebuild language options for C-Pointer
            if (state.languageDropdown) {
                state.languageDropdown.updateOptions([
                    {value: 'rust', label: 'Rust'},
                    {value: 'c', label: 'C/C++'},
                    {value: 'python', label: 'Python (ctypes)'},
                    {value: 'js', label: 'Node.js (ffi-napi)'}
                ], false);
            }
            
            if (resetPayload) {
                updateCustomSelect('custom-req-language', 'rust');
                onLanguageChange(true);
            }
            urlInput.value = 'cluaiz_engine_invoke(ptr)';
            urlInput.placeholder = "Function pointer / symbol name";


        }
    }

    export function onLanguageChange(resetPayload = true) {
        const protocol = state.protocolDropdown ? state.protocolDropdown.getValue() : 'http';
        const lang = state.languageDropdown ? state.languageDropdown.getValue() : 'rust';

        // Turn off JSON lint for non-JSON modes
        if (state.editor) state.editor.setOption("lint", false);

        if (protocol === 'http') {
            if (lang === 'json') {
                if (state.editor) state.editor.setOption("mode", "application/json");
                if (resetPayload && state.editor) state.editor.setValue('{\n  \n}');
            } else if (lang === 'cel') {
                if (state.editor) state.editor.setOption("mode", "rust");
                if (resetPayload && state.editor) state.editor.setValue("let $users = use plugin::database -> find User -> limit 5;\nforeach ($user in $users) {\n    use plugin::email -> send(to: $user.email);\n}");
            } else if (lang === 'rhai') {
                if (state.editor) state.editor.setOption("mode", "rust");
                if (resetPayload && state.editor) state.editor.setValue("fn process(data) {\n    return data + \"_processed\";\n}\nprocess(\"test\");");
            } else if (lang === 'wasm') {
                if (state.editor) state.editor.setOption("mode", "rust");
                if (resetPayload && state.editor) state.editor.setValue("(module\n  (func $main (result i32)\n    i32.const 42\n  )\n  (export \"main\" (func $main))\n)");
            } else if (lang === 'js') {
                if (state.editor) state.editor.setOption("mode", "javascript");
                if (resetPayload && state.editor) state.editor.setValue("function process(data) {\n  return data + \"_processed\";\n}\nprocess(\"test\");");
            }
        } else if (protocol === 'c-pointer') {
            if (lang === 'rust') {
                if (state.editor) state.editor.setOption("mode", "rust");
                if (resetPayload && state.editor) state.editor.setValue("#[repr(C)]\npub struct Payload {\n    pub id: u32,\n    pub data_ptr: *const u8,\n}");
            } else if (lang === 'c') {
                if (state.editor) state.editor.setOption("mode", "text/x-csrc");
                if (resetPayload && state.editor) state.editor.setValue("typedef struct {\n    uint32_t id;\n    const char* data_ptr;\n} Payload;");
            } else if (lang === 'python') {
                if (state.editor) state.editor.setOption("mode", "python");
                if (resetPayload && state.editor) state.editor.setValue("class Payload(ctypes.Structure):\n    _fields_ = [\n        (\"id\", ctypes.c_uint32),\n        (\"data_ptr\", ctypes.c_char_p)\n    ]");
            } else if (lang === 'js') {
                if (state.editor) state.editor.setOption("mode", "javascript");
                if (resetPayload && state.editor) state.editor.setValue("const StructType = require('ref-struct-napi');\n\nconst Payload = StructType({\n  id: 'uint32',\n  data_ptr: 'string'\n});");
            }

        }
    }

    export async function sendRequest() {
        if (!state.activeEndpoint) return;

        // Auto-expand response panel if it's minimized
        toggleResponsePanel(true);

        const ep = state.activeEndpoint;
        const btn = document.getElementById('btn-send');
        const url = document.getElementById('req-url').value;
        const method = state.methodDropdown ? state.methodDropdown.getValue() : 'GET';
        const protocol = state.protocolDropdown ? state.protocolDropdown.getValue() : 'http';
        const resBody = document.getElementById('res-body');
        const resStatus = document.getElementById('res-status');
        const bodyStr = state.editor ? state.editor.getValue() : "";

        resBody.textContent = "Sending request...";
        resBody.style.color = "#8b949e";
        btn.disabled = true;

        const options = {
            method: method,
            headers: {
                'Content-Type': 'application/json'
            }
        };

        try {
            const pRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
            if (pRes.ok) {
                const pData = await pRes.json();
                if (pData.permission && pData.permission.api_auth && pData.permission.api_auth.tokens && pData.permission.api_auth.tokens.length > 0) {
                    options.headers['Authorization'] = 'Bearer ' + pData.permission.api_auth.tokens[0];
                }
            }
        } catch (e) {
            console.warn("Could not fetch permissions for auth token");
        }

        if ((ep.method === 'POST' || ep.method === 'PUT' || ep.method === 'DELETE') && bodyStr.trim() !== '') {
            if (protocol === 'http') {
                const reqLang = state.languageDropdown ? state.languageDropdown.getValue() : 'json';
                if (reqLang === 'json') {
                    try {
                        JSON.parse(bodyStr); // Validate JSON
                        options.body = bodyStr;
                    } catch (e) {
                        resBody.textContent = "Invalid JSON in request payload:\n" + e.message;
                        resBody.style.color = "var(--method-delete)";
                        btn.disabled = false;
                        return;
                    }
                } else {
                    // It's a script (CEL, Rhai, WASM, JS) sent via HTTP REST
                    options.body = JSON.stringify({ lang: reqLang, script: bodyStr });
                }
            } else if (protocol === 'c-pointer') {
                // Wrap in generic params object for FFI APIs
                options.body = JSON.stringify({ params: bodyStr });
            } else {
                options.body = bodyStr;
            }
        }

        const start = performance.now();
        try {
            const response = await fetch(url, options);
            const time = (performance.now() - start).toFixed(2);
            const statusClass = response.ok ? 'status-ok' : 'status-err';

            const text = await response.text();
            let size = new Blob([text]).size;

            resStatus.innerHTML = `<span class="${statusClass}">Status: ${response.status} ${response.statusText}</span><span>Time: ${time} ms</span><span>Size: ${size} B</span>`;

            try {
                const json = JSON.parse(text);
                resBody.textContent = JSON.stringify(json, null, 2);
                resBody.style.color = "#a5d6ff";
            } catch (e) {
                resBody.textContent = text;
                resBody.style.color = "#a5d6ff";
            }
        } catch (e) {
            resStatus.innerHTML = `<span class="status-err">Error</span><span>Time: - ms</span><span>Size: - B</span>`;
            resBody.textContent = "Network error: " + e.message;
            resBody.style.color = "var(--method-delete)";
        }

        btn.disabled = false;
    }

