const state = {
    apiData: [],
    activeEndpoint: null,
    editor: null,
    headersEditor: null
};

document.addEventListener('DOMContentLoaded', async () => {
    try {
        const res = await fetch('/api_data.json');
        state.apiData = await res.json();
        renderSidebar();
        renderHome();
        app.setupCustomSelects();
        app.initEditor();
        app.initResizer();
        app.initSidebarResizer();
    } catch (e) {
        console.error("Failed to load API data:", e);
    }
});

function renderSidebar() {
    const navContainer = document.getElementById('nav-container');
    navContainer.innerHTML = '';

    state.apiData.forEach((group, groupIdx) => {
        const groupEl = document.createElement('div');
        groupEl.className = 'nav-group';
        
        const titleEl = document.createElement('div');
        titleEl.className = 'nav-group-title';
        titleEl.innerHTML = `<span>${group.group}</span> <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>`;
        
        const itemsEl = document.createElement('div');
        itemsEl.className = 'nav-items';
        
        titleEl.onclick = () => {
            const isOpen = itemsEl.classList.contains('open');
            itemsEl.classList.toggle('open');
            if(!isOpen) {
                titleEl.classList.add('open');
            } else {
                titleEl.classList.remove('open');
            }
        };

        group.endpoints.forEach((ep) => {
            const link = document.createElement('div');
            link.className = 'nav-link';
            const methodClass = 'method-' + ep.method.toLowerCase();
            link.innerHTML = `<span class="method-badge ${methodClass}">${ep.method}</span> <span style="white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">${ep.path}</span>`;
            
            link.onclick = () => {
                document.querySelectorAll('.nav-link').forEach(l => l.classList.remove('active'));
                link.classList.add('active');
                openEndpoint(ep);
            };

            itemsEl.appendChild(link);
        });

        if(groupIdx === 0) {
            itemsEl.classList.add('open');
            titleEl.classList.add('open');
        }

        groupEl.appendChild(titleEl);
        groupEl.appendChild(itemsEl);
        navContainer.appendChild(groupEl);
    });
}

function renderHome() {
    document.getElementById('view-dashboard').classList.remove('hidden');
    document.getElementById('view-api').classList.add('hidden');
    document.querySelectorAll('.nav-link').forEach(l => l.classList.remove('active'));
}

function openEndpoint(ep) {
    state.activeEndpoint = ep;
    document.getElementById('view-dashboard').classList.add('hidden');
    document.getElementById('view-api').classList.remove('hidden');

    // Update Custom Select Value for Method
    app.updateCustomSelect('custom-req-method', ep.method);

    // Calculate Available Methods for this Group
    let currentGroup = state.apiData.find(g => g.endpoints.includes(ep));
    if (currentGroup) {
        const availableMethods = currentGroup.endpoints.map(e => e.method);
        app.updateAvailableMethods('custom-req-method', availableMethods);
    }
    
    document.getElementById('req-url').value = `http://localhost:8000${ep.path}`;
    
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

    app.updateCustomSelect('custom-req-protocol', targetProtocol);
    app.onProtocolChange(false); // Trigger UI update without clearing default payload
    
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
                if (state.editor) state.editor.setOption("readOnly", false);
            } else {
                if (state.editor) state.editor.setValue("{}");
                if (state.editor) state.editor.setOption("readOnly", false);
            }
        }
    } else {
        if (state.editor) state.editor.setValue("");
        if (state.editor) state.editor.setOption("readOnly", "nocursor");
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

    app.switchTab('params');

    // Reset response
    document.getElementById('res-body').textContent = "Hit \"Send\" to execute the request.";
    document.getElementById('res-body').style.color = "#8b949e";
    document.getElementById('res-status').innerHTML = "<span>Status: -</span><span>Time: - ms</span><span>Size: - B</span>";
}

window.app = {
    renderHome,
    switchTab(tab) {
        const tabParams = document.getElementById('tab-params');
        const tabHeaders = document.getElementById('tab-headers');
        const tabDocs = document.getElementById('tab-docs');
        const tabTerminal = document.getElementById('tab-terminal');
        const panelParams = document.getElementById('panel-params');
        const panelHeaders = document.getElementById('panel-headers');
        const panelDocs = document.getElementById('panel-docs');
        const panelTerminal = document.getElementById('panel-terminal');
        const panelTitle = document.getElementById('panel-left-title');

        [tabParams, tabHeaders, tabDocs, tabTerminal].forEach(t => t && t.classList.remove('active'));
        [panelParams, panelHeaders, panelDocs, panelTerminal].forEach(p => p && p.classList.add('hidden'));

        if (tab === 'params') {
            tabParams.classList.add('active');
            panelParams.classList.remove('hidden');
            panelTitle.textContent = "Request Payload";
            if (state.editor) setTimeout(() => state.editor.refresh(), 10);
        } else if (tab === 'headers') {
            tabHeaders.classList.add('active');
            panelHeaders.classList.remove('hidden');
            panelTitle.textContent = "Request Headers";
            if (state.headersEditor) setTimeout(() => state.headersEditor.refresh(), 10);
        } else if (tab === 'docs') {
            tabDocs.classList.add('active');
            panelDocs.classList.remove('hidden');
            panelTitle.textContent = "Documentation";
        } else if (tab === 'terminal') {
            if(tabTerminal) tabTerminal.classList.add('active');
            if(panelTerminal) panelTerminal.classList.remove('hidden');
            panelTitle.textContent = "Secure Web Terminal";
            const input = document.getElementById('terminal-input');
            if(input) input.focus();
        }
    },
    
    async handleTerminalKeyPress(e) {
        if (e.key === 'Enter') {
            const input = document.getElementById('terminal-input');
            const output = document.getElementById('terminal-output');
            const command = input.value.trim();
            if (!command) return;
            
            // Echo command
            output.innerHTML += `\n<span style="color: #6a8759">➜</span> <span style="color: #6897bb">cluaiz</span> ${command}\n`;
            input.value = '';
            
            // Check if it's a chat command for streaming
            if (command.startsWith('cluaiz chat ')) {
                const message = command.substring(13).replace(/^["']|["']$/g, '');
                try {
                    const response = await fetch('/v1/chat/stream', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ message: message })
                    });
                    
                    if (!response.ok) {
                        output.innerHTML += `<span style="color: #cc6666">Error: HTTP ${response.status}</span>`;
                        return;
                    }
                    
                    const reader = response.body.getReader();
                    const decoder = new TextDecoder("utf-8");
                    
                    while (true) {
                        const { done, value } = await reader.read();
                        if (done) break;
                        
                        const chunk = decoder.decode(value, { stream: true });
                        const lines = chunk.split('\n');
                        for (let line of lines) {
                            if (line.startsWith('data: ')) {
                                const data = line.substring(6);
                                output.innerHTML += data.replace(/</g, "&lt;").replace(/>/g, "&gt;");
                                output.scrollTop = output.scrollHeight;
                            }
                        }
                    }
                    output.innerHTML += "\n";
                } catch (err) {
                    output.innerHTML += `<span style="color: #cc6666">Error streaming chat: ${err.message}</span>\n`;
                }
            } else {
                // Generic CMD execution (Secure Local-Only)
                try {
                    const response = await fetch('/v1/system/cmd', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ command: command })
                    });
                    const resJson = await response.json();
                    
                    if (resJson.status === 'success') {
                        output.innerHTML += resJson.output.replace(/</g, "&lt;").replace(/>/g, "&gt;") + "\n";
                    } else {
                        output.innerHTML += `<span style="color: #cc6666">${resJson.output || resJson.message}</span>\n`;
                    }
                } catch (err) {
                    output.innerHTML += `<span style="color: #cc6666">Error executing command: ${err.message}</span>\n`;
                }
            }
            output.scrollTop = output.scrollHeight;
        }
    },
    initEditor() {
        const textArea = document.getElementById('req-body');
        state.editor = CodeMirror.fromTextArea(textArea, {
            mode: "application/json",
            theme: "darcula",
            lineNumbers: true,
            gutters: ["CodeMirror-lint-markers"],
            lint: true,
            indentUnit: 2,
            matchBrackets: true,
            autoCloseBrackets: true
        });
        state.editor.setSize("100%", "100%");

        const headerArea = document.getElementById('req-headers');
        state.headersEditor = CodeMirror.fromTextArea(headerArea, {
            mode: "application/json",
            theme: "darcula",
            lineNumbers: true,
            gutters: ["CodeMirror-lint-markers"],
            lint: true,
            indentUnit: 2,
            matchBrackets: true,
            autoCloseBrackets: true
        });
        state.headersEditor.setSize("100%", "100%");

        setTimeout(() => {
            if(state.editor) state.editor.refresh();
            if(state.headersEditor) state.headersEditor.refresh();
        }, 100);
    },
    initResizer() {
        const resizer = document.getElementById('drag-resizer');
        const topPanel = document.getElementById('panel-top-container');
        let isDragging = false;

        resizer.addEventListener('mousedown', function(e) {
            isDragging = true;
            document.body.style.cursor = 'ns-resize';
            resizer.classList.add('dragging');
        });

        document.addEventListener('mousemove', function(e) {
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

        document.addEventListener('mouseup', function(e) {
            if (!isDragging) return;
            isDragging = false;
            document.body.style.cursor = 'default';
            resizer.classList.remove('dragging');
            if (state.editor) state.editor.refresh();
        });
    },
    initSidebarResizer() {
        const resizer = document.getElementById('sidebar-resizer');
        const sidebar = document.getElementById('sidebar');
        let isDraggingSidebar = false;

        resizer.addEventListener('mousedown', function(e) {
            isDraggingSidebar = true;
            document.body.style.cursor = 'ew-resize';
            resizer.classList.add('dragging');
        });

        document.addEventListener('mousemove', function(e) {
            if (!isDraggingSidebar) return;
            let newWidth = e.clientX;
            // set min and max width constraints
            if (newWidth < 150) newWidth = 150;
            if (newWidth > 600) newWidth = 600;
            sidebar.style.width = `${newWidth}px`;
        });

        document.addEventListener('mouseup', function(e) {
            if (!isDraggingSidebar) return;
            isDraggingSidebar = false;
            document.body.style.cursor = 'default';
            resizer.classList.remove('dragging');
            if (state.editor) state.editor.refresh();
        });
    },
    toggleSidebar() {
        const sidebar = document.getElementById('sidebar');
        const resizer = document.getElementById('sidebar-resizer');
        const openBtnApi = document.getElementById('sidebar-open-btn-api');
        const openBtnDashboard = document.getElementById('sidebar-open-btn-dashboard');
        const isHidden = sidebar.classList.contains('hidden');
        
        if (isHidden) {
            sidebar.classList.remove('hidden');
            resizer.classList.remove('hidden');
            if(openBtnApi) openBtnApi.classList.add('hidden');
            if(openBtnDashboard) openBtnDashboard.classList.add('hidden');
        } else {
            sidebar.classList.add('hidden');
            resizer.classList.add('hidden');
            if(openBtnApi) openBtnApi.classList.remove('hidden');
            if(openBtnDashboard) openBtnDashboard.classList.remove('hidden');
        }
        
        setTimeout(() => {
            if (state.editor) state.editor.refresh();
        }, 50);
    },
    toggleResponsePanel(forceOpen = false) {
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
    },
    setupCustomSelects() {
        const x = document.getElementsByClassName("custom-select");
        for (let i = 0; i < x.length; i++) {
            const selElmnt = x[i];
            const selectedDiv = selElmnt.querySelector(".select-selected");
            const itemsDiv = selElmnt.querySelector(".select-items");
            
            // Remove old event listeners if calling again
            const clone = selectedDiv.cloneNode(true);
            selectedDiv.parentNode.replaceChild(clone, selectedDiv);
            
            clone.addEventListener("click", function(e) {
                e.stopPropagation();
                app.closeAllSelect(this);
                this.nextElementSibling.classList.toggle("select-hide");
                this.classList.toggle("select-arrow-active");
            });

            const items = itemsDiv.querySelectorAll("div");
            items.forEach(item => {
                item.onclick = function(e) {
                    if (this.classList.contains("disabled-option")) return;
                    
                    const parentSelect = this.parentNode.parentNode;
                    const selDisplay = parentSelect.querySelector(".select-selected");
                    
                    selDisplay.innerHTML = this.innerHTML;
                    parentSelect.setAttribute('data-value', this.getAttribute('data-value'));
                    
                    // Mark selected
                    const y = this.parentNode.querySelectorAll(".same-as-selected");
                    for (let k = 0; k < y.length; k++) {
                        y[k].classList.remove("same-as-selected");
                    }
                    this.classList.add("same-as-selected");
                    selDisplay.click(); // close

                    // Trigger logical changes
                    if (parentSelect.id === 'custom-req-protocol') {
                        app.onProtocolChange();
                    } else if (parentSelect.id === 'custom-req-language') {
                        app.onLanguageChange();
                    } else if (parentSelect.id === 'custom-req-method') {
                        app.onMethodChange(this.getAttribute('data-value'));
                    }
                };
            });
        }
        
        document.addEventListener("click", app.closeAllSelect);
    },
    closeAllSelect(elmnt) {
        const x = document.getElementsByClassName("select-items");
        const y = document.getElementsByClassName("select-selected");
        const arrNo = [];
        for (let i = 0; i < y.length; i++) {
            if (elmnt == y[i]) {
                arrNo.push(i);
            } else {
                y[i].classList.remove("select-arrow-active");
            }
        }
        for (let i = 0; i < x.length; i++) {
            if (arrNo.indexOf(i)) {
                x[i].classList.add("select-hide");
            }
        }
    },
    updateCustomSelect(id, val) {
        const select = document.getElementById(id);
        if (!select) return;
        select.setAttribute('data-value', val);
        const display = select.querySelector('.select-selected');
        const items = select.querySelectorAll('.select-items div');
        items.forEach(item => {
            item.classList.remove('same-as-selected');
            if (item.getAttribute('data-value') === val) {
                display.innerHTML = item.innerHTML;
                item.classList.add('same-as-selected');
            }
        });
    },
    updateAvailableMethods(id, availableMethods) {
        const select = document.getElementById(id);
        if (!select) return;
        const items = select.querySelectorAll('.select-items div');
        items.forEach(item => {
            const val = item.getAttribute('data-value');
            if (availableMethods.includes(val)) {
                item.classList.remove('disabled-option');
            } else {
                item.classList.add('disabled-option');
            }
        });
    },
    onMethodChange(newMethod) {
        if (!state.activeEndpoint) return;
        const currentGroup = state.apiData.find(g => g.endpoints.includes(state.activeEndpoint));
        if (!currentGroup) return;

        // Find the first endpoint in the same group that matches the new method
        const targetEp = currentGroup.endpoints.find(e => e.method === newMethod);
        if (targetEp) {
            // Update sidebar selection visually
            document.querySelectorAll('.nav-link').forEach(l => {
                l.classList.remove('active');
                if (l.innerText.includes(targetEp.path) && l.innerText.includes(targetEp.method)) {
                    l.classList.add('active');
                }
            });
            openEndpoint(targetEp);
        }
    },
    onProtocolChange(resetPayload = true) {
        const protocol = document.getElementById('custom-req-protocol').getAttribute('data-value');
        const methodSelect = document.getElementById('custom-req-method');
        const langSelect = document.getElementById('custom-req-language');
        const urlInput = document.getElementById('req-url');

        // Reset visibility
        methodSelect.classList.remove('hidden');
        langSelect.classList.add('hidden');
        if (state.editor) state.editor.setOption("readOnly", false);

        if (protocol === 'http') {
            if (state.editor) {
                state.editor.setOption("mode", "application/json");
                state.editor.setOption("lint", true);
            }
            if(resetPayload) {
                if (state.editor) state.editor.setValue("{\n  \n}");
                if (state.activeEndpoint) {
                    urlInput.value = `http://localhost:8000${state.activeEndpoint.path}`;
                } else {
                    urlInput.value = "http://localhost:8000/";
                }
            }
        } else if (protocol === 'c-pointer') {
            methodSelect.classList.add('hidden');
            langSelect.classList.remove('hidden');
            
            const itemsDiv = langSelect.querySelector('.select-items');
            itemsDiv.innerHTML = `
                <div data-value="rust">Rust</div>
                <div data-value="c">C/C++</div>
                <div data-value="python">Python (ctypes)</div>
                <div data-value="js">Node.js (ffi-napi)</div>
            `;
            app.setupCustomSelects(); // Re-bind clicks for new items
            app.updateCustomSelect('custom-req-language', 'rust');

            if(resetPayload) {
                urlInput.value = "0x000000000000";
            }
            app.onLanguageChange(resetPayload);
        } else if (protocol === 'cel') {
            methodSelect.classList.add('hidden');
            langSelect.classList.remove('hidden');
            
            const itemsDiv = langSelect.querySelector('.select-items');
            itemsDiv.innerHTML = `
                <div data-value="cel">CEL (cluaiz Engine Language)</div>
                <div data-value="rhai">Rhai Script</div>
                <div data-value="wasm">WASM (Rust)</div>
                <div data-value="js">JavaScript (V8)</div>
            `;
            app.setupCustomSelects(); // Re-bind clicks for new items
            app.updateCustomSelect('custom-req-language', 'cel');

            if(resetPayload) {
                urlInput.value = "cel://local/executor";
            }
            app.onLanguageChange(resetPayload);
        }
    },
    onLanguageChange(resetPayload = true) {
        const protocol = document.getElementById('custom-req-protocol').getAttribute('data-value');
        const lang = document.getElementById('custom-req-language').getAttribute('data-value');

        // Turn off JSON lint for non-JSON modes
        if (state.editor) state.editor.setOption("lint", false);

        if (protocol === 'c-pointer') {
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
        } else if (protocol === 'cel') {
            if (lang === 'cel') {
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
        }
    },
    async sendRequest() {
        if(!state.activeEndpoint) return;
        
        // Auto-expand response panel if it's minimized
        app.toggleResponsePanel(true);

        const ep = state.activeEndpoint;
        const url = document.getElementById('req-url').value;
        const bodyStr = state.editor ? state.editor.getValue() : "";
        const resBody = document.getElementById('res-body');
        const resStatus = document.getElementById('res-status');
        const btn = document.getElementById('btn-send');

        resBody.textContent = "Sending request...";
        resBody.style.color = "#8b949e";
        btn.disabled = true;

        const protocol = document.getElementById('custom-req-protocol').getAttribute('data-value');

        const options = {
            method: ep.method,
            headers: {
                'Content-Type': 'application/json'
            }
        };

        if ((ep.method === 'POST' || ep.method === 'PUT' || ep.method === 'DELETE') && bodyStr.trim() !== '') {
            if (protocol === 'http') {
                try {
                    JSON.parse(bodyStr); // Validate JSON
                    options.body = bodyStr;
                } catch (e) {
                    resBody.textContent = "Invalid JSON in request payload:\n" + e.message;
                    resBody.style.color = "var(--method-delete)";
                    btn.disabled = false;
                    return;
                }
            } else if (protocol === 'cel') {
                // Wrap raw code in script field for CEL API
                options.body = JSON.stringify({ script: bodyStr });
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
};
