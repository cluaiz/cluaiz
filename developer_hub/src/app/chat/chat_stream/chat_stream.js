// Conversation history for context
const conversationHistory = [];

window.copyCodeBlock = function(btn, encodedCode) {
    try {
        const decoded = decodeURIComponent(atob(encodedCode));
        navigator.clipboard.writeText(decoded).then(() => {
            const originalHTML = btn.innerHTML;
            btn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#4ade80" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg> Copied!';
            btn.style.color = '#4ade80';
            setTimeout(() => {
                btn.innerHTML = originalHTML;
                btn.style.color = '#9ca3af';
            }, 2000);
        });
    } catch (e) {
        console.error('Failed to copy code:', e);
    }
};

export async function mountChatStream(rootElement) {
    try {
        const response = await fetch('/src/app/chat/chat_stream/chat_stream.html?v=' + new Date().getTime());
        const html = await response.text();
        rootElement.innerHTML = html;

        if (rootElement.id !== 'chat-stream-mount-point') {
            const header = rootElement.querySelector('#chat-header');
            if (header) {
                header.remove();
            }
        }

        // Add CSS if not already present
        if (!document.getElementById('chat-stream-css')) {
            const link = document.createElement('link');
            link.id = 'chat-stream-css';
            link.rel = 'stylesheet';
            link.href = '/src/app/chat/chat_stream/chat_stream.css?v=' + new Date().getTime();
            document.head.appendChild(link);
        }

        setupChatStream();
    } catch (e) {
        rootElement.innerHTML = `<h2 style="color:red; padding: 20px;">Failed to load chat stream: ${e.message}</h2>`;
    }
}

function getSelectedModel() {
    const modelText = document.getElementById('selected-model-text');
    return modelText ? modelText.textContent.trim() : 'default';
}

function setupChatStream() {
    // Only register the event listener once globally
    window.addEventListener('chat:cancel', () => {
        // Remove the last AI and User message from history
        if (conversationHistory.length >= 2) {
            conversationHistory.pop(); // Remove AI
            conversationHistory.pop(); // Remove User
        } else if (conversationHistory.length === 1) {
            conversationHistory.pop();
        }
        
        // Remove the last two messages from DOM
        const container = document.getElementById('chat-stream-container');
        const messages = container.querySelectorAll('.chat-message');
        if (messages.length > 0) {
            messages[messages.length - 1].remove(); // AI bubble
        }
        if (messages.length > 1) {
            messages[messages.length - 2].remove(); // User bubble
        }
        window.canContinue = false;
        
        // If chat is empty now, we can leave the chat UI active
        // so the user doesn't get kicked out to the dashboard.
        if (conversationHistory.length === 0) {
            // Do not hide the stream UI or restore dashboard here
        }
    });

    // Check if the current URL is /chat on initial load
    if (window.location.pathname === '/chat') {
        const container = document.getElementById('chat-stream-container');
        if (container && !container.classList.contains('active')) {
            container.classList.add('active');
            const header = document.getElementById('chat-header');
            if (header) header.style.display = 'flex';
            
            const dashboardHero = document.querySelector('.dashboard-hero');
            const dashboardMain = document.querySelector('.dashboard-main');
            const topBar = document.querySelector('.top-bar');
            if (dashboardHero) dashboardHero.style.display = 'none';
            if (dashboardMain) dashboardMain.style.display = 'none';
            if (topBar) topBar.style.display = 'none';
        }
    }

    if (!window.chatSendHandler) {
        window.chatSendHandler = (e) => {
            const container = document.getElementById('chat-stream-container');
            const content = e.detail?.message;
            if (!content || !container) return;

            // Activate the stream container (show it)
            if (!container.classList.contains('active')) {
                container.classList.add('active');
                
                // Change URL to /chat
                if (window.location.pathname !== '/chat') {
                    window.history.pushState({}, '', '/chat');
                }

                // Hide dashboard content and show header
                const header = document.getElementById('chat-header');
                if (header) header.style.display = 'flex';
                
                const dashboardHero = document.querySelector('.dashboard-hero');
                const dashboardMain = document.querySelector('.dashboard-main');
                const topBar = document.querySelector('.top-bar');
                if (dashboardHero) dashboardHero.style.display = 'none';
                if (dashboardMain) dashboardMain.style.display = 'none';
                if (topBar) topBar.style.display = 'none';
            }

            // Render user message
            appendMessage(content, 'user');

            // Add to conversation history
            conversationHistory.push({ role: 'user', content: content });

            // Scroll to bottom
            container.scrollTop = container.scrollHeight;

            // Send to AI endpoint
            const think_mode = e.detail?.think_mode;
            const response_length = e.detail?.response_length;
            sendToAI(content, think_mode, response_length);
        };
        window.addEventListener('chat:send', window.chatSendHandler);
        
        // Scroll handler to hide header
        const streamContainer = document.getElementById('chat-stream-container');
        if (streamContainer) {
            let lastScrollTop = streamContainer.scrollTop;
            streamContainer.addEventListener('scroll', () => {
                const header = document.getElementById('chat-header');
                if (header) {
                    let currentScrollTop = streamContainer.scrollTop;
                    if (currentScrollTop > lastScrollTop && currentScrollTop > 10) {
                        // Scrolling down
                        header.style.opacity = '0';
                        header.style.pointerEvents = 'none';
                    } else if (currentScrollTop < lastScrollTop || currentScrollTop <= 10) {
                        // Scrolling up or at absolute top
                        header.style.opacity = '1';
                        header.style.pointerEvents = 'auto';
                    }
                    lastScrollTop = currentScrollTop <= 0 ? 0 : currentScrollTop;
                }
            });
        }
        
        // Setup Chat Header Actions
        const backBtn = document.getElementById('chat-back-btn');
        if (backBtn && !backBtn.dataset.bound) {
            backBtn.addEventListener('click', () => {
                const container = document.getElementById('chat-stream-container');
                const header = document.getElementById('chat-header');
                container.classList.remove('active');
                if (header) header.style.display = 'none';

                const dashboardHero = document.querySelector('.dashboard-hero');
                
                // Revert URL to /
                if (window.location.pathname === '/chat') {
                    window.history.pushState({}, '', '/');
                }
                const dashboardMain = document.querySelector('.dashboard-main');
                const topBar = document.querySelector('.top-bar');
                if (dashboardHero) dashboardHero.style.display = '';
                if (dashboardMain) dashboardMain.style.display = '';
                if (topBar) topBar.style.display = '';
            });
            backBtn.dataset.bound = "true";
        }

        const menuBtn = document.getElementById('chat-menu-btn');
        const dropdown = document.getElementById('chat-menu-dropdown');
        if (menuBtn && dropdown && !menuBtn.dataset.bound) {
            menuBtn.addEventListener('click', async (e) => {
                e.stopPropagation();
                dropdown.classList.toggle('show');
                
                if (dropdown.classList.contains('show')) {
                    try {
                        let headers = {};
                        let pData = null;
                        const pRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
                        if (pRes.ok) {
                            pData = await pRes.json();
                            if (pData.permission && pData.permission.api_auth && pData.permission.api_auth.tokens && pData.permission.api_auth.tokens.length > 0) {
                                headers['Authorization'] = 'Bearer ' + pData.permission.api_auth.tokens[0];
                            }
                        }

                        const res = await fetch(window.getApiBaseUrl() + '/v1/system/ps', { headers });
                        if (res.ok) {
                            const data = await res.json();
                            if (data.active_processes && data.active_processes.length > 0) {
                                const proc = data.active_processes[0];
                                document.getElementById('info-model-id').textContent = `Model: ${proc.model_id || 'Unknown'}`;
                                document.getElementById('info-context-size').textContent = `Context: ${proc.context_size || '?'} / ${proc.original_context || '?'}`;
                                
                                const unloadBtn = document.getElementById('unload-model-btn');
                                const divider = document.getElementById('chat-session-divider');
                                const sessionInfo = document.getElementById('chat-session-info');
                                const viewHeaderBtn = document.getElementById('view-model-header-btn');
                                
                                if (unloadBtn) unloadBtn.style.display = 'flex';
                                if (divider) divider.style.display = 'block';
                                if (sessionInfo) sessionInfo.style.display = 'flex';
                                
                                // Only show Model Header option if permission is ON and model is active
                                if (pData && pData.permission && pData.permission.model_header_info === true) {
                                    if (viewHeaderBtn) viewHeaderBtn.style.display = 'flex';
                                    window.currentModelProc = proc;
                                } else {
                                    if (viewHeaderBtn) viewHeaderBtn.style.display = 'none';
                                }
                            } else {
                                const unloadBtn = document.getElementById('unload-model-btn');
                                const divider = document.getElementById('chat-session-divider');
                                const sessionInfo = document.getElementById('chat-session-info');
                                const viewHeaderBtn = document.getElementById('view-model-header-btn');
                                if (unloadBtn) unloadBtn.style.display = 'none';
                                if (divider) divider.style.display = 'none';
                                if (sessionInfo) sessionInfo.style.display = 'none';
                                if (viewHeaderBtn) viewHeaderBtn.style.display = 'none';
                            }
                        } else {
                            const unloadBtn = document.getElementById('unload-model-btn');
                            const divider = document.getElementById('chat-session-divider');
                            const sessionInfo = document.getElementById('chat-session-info');
                            const viewHeaderBtn = document.getElementById('view-model-header-btn');
                            if (unloadBtn) unloadBtn.style.display = 'none';
                            if (divider) divider.style.display = 'none';
                            if (sessionInfo) sessionInfo.style.display = 'none';
                            if (viewHeaderBtn) viewHeaderBtn.style.display = 'none';
                        }
                    } catch (err) {
                        console.error('Failed to fetch system ps:', err);
                        const unloadBtn = document.getElementById('unload-model-btn');
                        const divider = document.getElementById('chat-session-divider');
                        const sessionInfo = document.getElementById('chat-session-info');
                        const viewHeaderBtn = document.getElementById('view-model-header-btn');
                        if (unloadBtn) unloadBtn.style.display = 'none';
                        if (divider) divider.style.display = 'none';
                        if (sessionInfo) sessionInfo.style.display = 'none';
                        if (viewHeaderBtn) viewHeaderBtn.style.display = 'none';
                    }
                }
            });
            document.addEventListener('click', () => {
                dropdown.classList.remove('show');
            });
            menuBtn.dataset.bound = "true";
        }

        const exportBtn = document.getElementById('export-md-btn');
        if (exportBtn && !exportBtn.dataset.bound) {
            exportBtn.addEventListener('click', () => {
                if (conversationHistory.length === 0) return;
                let mdContent = "# Chat Export\n\n";
                for (let msg of conversationHistory) {
                    const role = msg.role === 'user' ? 'User' : 'Cluaiz Engine';
                    mdContent += `### ${role}\n${msg.content}\n\n---\n\n`;
                }
                const blob = new Blob([mdContent], { type: 'text/markdown' });
                const url = URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.href = url;
                a.download = `chat_export_${new Date().getTime()}.md`;
                document.body.appendChild(a);
                a.click();
                document.body.removeChild(a);
                URL.revokeObjectURL(url);
            });
            exportBtn.dataset.bound = "true";
        }

        const viewHeaderBtn = document.getElementById('view-model-header-btn');
        if (viewHeaderBtn && !viewHeaderBtn.dataset.bound) {
            const modal = document.getElementById('model-header-modal');
            const closeBtn = document.getElementById('close-header-modal');
            const content = document.getElementById('model-header-content');
            
            viewHeaderBtn.addEventListener('click', () => {
                if (window.currentModelProc && Array.isArray(window.currentModelProc)) {
                    content.innerHTML = `<span style="color: #60a5fa; font-weight: bold; font-size: 1.1rem;">[Active Engine Instances]</span>\n\n`;
                    
                    if (window.currentModelProc.length === 0) {
                        content.innerHTML += `<span style="color: #9ca3af;">No active models loaded in memory.</span>`;
                    }
                    
                    window.currentModelProc.forEach((proc, index) => {
                        content.innerHTML += `<div style="margin-bottom: 15px; padding: 10px; border: 1px solid #374151; border-radius: 6px; background: rgba(17, 24, 39, 0.5);">`;
                        content.innerHTML += `<span style="color: #34d399; font-weight: bold;">Engine ${index + 1}: ${proc.engine || 'Unknown'}</span>\n`;
                        content.innerHTML += `<span style="color: #a78bfa;">Model ID:</span>      ${proc.model_id || 'N/A'}\n`;
                        content.innerHTML += `<span style="color: #a78bfa;">Status:</span>        ${proc.status || 'N/A'}\n`;
                        content.innerHTML += `<span style="color: #a78bfa;">Memory:</span>        ${proc.memory_usage_mb ? proc.memory_usage_mb + (typeof proc.memory_usage_mb === 'number' ? ' MB' : '') : 'N/A'}\n`;
                        content.innerHTML += `<span style="color: #a78bfa;">Context Used:</span>  ${proc.context_size || '0'} tokens\n`;
                        content.innerHTML += `<span style="color: #a78bfa;">Context Total:</span> ${proc.original_context || '0'} tokens\n`;
                        
                        if (proc.is_gguf) {
                            content.innerHTML += `<span style="color: #a78bfa;">Format:</span>          GGUF (Quantized)\n`;
                        } else if (proc.is_onnx) {
                            content.innerHTML += `<span style="color: #a78bfa;">Format:</span>          ONNX (Accelerated)\n`;
                        }
                        
                        if (proc.raw_header && Object.keys(proc.raw_header).length > 0) {
                            content.innerHTML += `\n<span style="color: #9ca3af; font-size: 0.75rem;">Metadata:</span>\n`;
                            content.innerHTML += `<span style="color: #6b7280; font-size: 0.75rem;">${JSON.stringify(proc.raw_header, null, 2)}</span>`;
                        }
                        content.innerHTML += `</div>`;
                    });
                    
                    modal.style.display = 'flex';
                    const dropdown = document.getElementById('chat-menu-dropdown');
                    if (dropdown) dropdown.classList.remove('show');
                }
            });
            
            closeBtn.addEventListener('click', () => {
                modal.style.display = 'none';
            });
            
            modal.addEventListener('click', (e) => {
                if (e.target === modal) modal.style.display = 'none';
            });
            
            viewHeaderBtn.dataset.bound = "true";
        }

        const unloadBtn = document.getElementById('unload-model-btn');
        if (unloadBtn && !unloadBtn.dataset.bound) {
            unloadBtn.addEventListener('click', async () => {
                try {
                    let headers = {};
                    const pRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
                    if (pRes.ok) {
                        const pData = await pRes.json();
                        if (pData.permission && pData.permission.api_auth && pData.permission.api_auth.tokens && pData.permission.api_auth.tokens.length > 0) {
                            headers['Authorization'] = 'Bearer ' + pData.permission.api_auth.tokens[0];
                        }
                    }

                    unloadBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="animation: spin 1s linear infinite;"><path d="M21 12a9 9 0 1 1-6.219-8.56"></path></svg> Unloading...';
                    
                    const res = await fetch(window.getApiBaseUrl() + '/v1/chat/completions', { 
                        method: 'POST', 
                        headers: {
                            'Content-Type': 'application/json',
                            ...headers
                        },
                        body: JSON.stringify({
                            model: getSelectedModel(),
                            messages: [],
                            keep_alive: 0
                        })
                    });
                    if (res.ok) {
                        unloadBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#22c55e" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6L9 17l-5-5"></path></svg> <span style="color: #22c55e">Unloaded</span>';
                        setTimeout(() => {
                            unloadBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3h18v18H3zM15 9l-6 6M9 9l6 6"/></svg> Unload Model';
                        }, 2000);
                        
                        // Force update of PS details if menu is open
                        if (dropdown.classList.contains('show')) {
                            menuBtn.click();
                            setTimeout(() => menuBtn.click(), 100);
                        }
                    }
                } catch (err) {
                    console.error('Failed to unload model:', err);
                    unloadBtn.innerHTML = '<span style="color: #ef4444">Failed</span>';
                    setTimeout(() => {
                        unloadBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3h18v18H3zM15 9l-6 6M9 9l6 6"/></svg> Unload Model';
                    }, 2000);
                }
            });
            unloadBtn.dataset.bound = "true";
        }
    }
}

async function sendToAI(userMessage, think_mode, response_length) {
    const container = document.getElementById('chat-stream-container');
    const model = getSelectedModel();

    const aiMsgEl = document.createElement('div');
    aiMsgEl.className = 'chat-message ai-message';
    aiMsgEl.innerHTML = `
        <div class="message-bubble ai-bubble" style="display: flex; flex-direction: column; gap: 8px;">
            <div class="status-container" style="font-family: monospace; font-size: 0.75rem; color: #9ca3af; display: flex; flex-direction: column; gap: 4px;">
                <div class="engine-loader" style="display: flex; align-items: center;">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" style="margin-right: 8px; animation: spin 1s linear infinite; transform-origin: center;">
                        <path d="M21 12a9 9 0 1 1-6.219-8.56"></path>
                    </svg>
                    <span>Warming up Cluaiz Engine...</span>
                </div>
            </div>
            <div class="tools-container" style="display: flex; flex-direction: column; gap: 8px; margin-top: 5px;"></div>
            <div class="divider" style="border-top: 1px solid rgba(156, 163, 175, 0.2); display: none;"></div>
            <div class="final-text markdown-body" style="font-size: 0.9rem;"></div>
        </div>
    `;
    container.appendChild(aiMsgEl);
    container.scrollTop = container.scrollHeight;

    const statusContainer = aiMsgEl.querySelector('.status-container');
    const divider = aiMsgEl.querySelector('.divider');
    const aiTextEl = aiMsgEl.querySelector('.final-text');
    let hasStarted = false;
    let isThinking = false;
    let skipThinking = false;
    let fullContent = '';

    const onSkipThinking = () => {
        skipThinking = true;
    };
    window.addEventListener('chat:skip_thinking', onSkipThinking);

    const updateStatus = (text) => {
        statusContainer.style.display = 'flex';
        statusContainer.innerHTML = `
            <div style="display: flex; align-items: center; color: #22c55e;">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" style="margin-right: 8px; animation: spin 1s linear infinite; transform-origin: center;">
                    <path d="M21 12a9 9 0 1 1-6.219-8.56"></path>
                </svg>
                <span>${escapeHtml(text)}</span>
            </div>
        `;
        container.scrollTop = container.scrollHeight;
    };

    window.currentChatController = new AbortController();
    let isAborted = false;

    const onAbort = () => {
        window.currentChatController.abort();
    };
    window.addEventListener('chat:abort', onAbort);

    window.dispatchEvent(new CustomEvent('chat:start'));

    try {
        const pRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
        let authToken = null;
        if (pRes.ok) {
            const pData = await pRes.json();
            if (pData.permission && pData.permission.api_auth && pData.permission.api_auth.tokens && pData.permission.api_auth.tokens.length > 0) {
                authToken = pData.permission.api_auth.tokens[0];
            }
        }
        
        const headers = { 'Content-Type': 'application/json' };
        if (authToken) {
            headers['Authorization'] = 'Bearer ' + authToken;
        }

        const payload = {
            model: model,
            messages: conversationHistory,
            stream: true
        };
        if (think_mode) payload.think_mode = think_mode;
        if (response_length) payload.response_length = response_length;

        const response = await fetch(window.getApiBaseUrl() + '/v1/chat/completions', {
            method: 'POST',
            headers: headers,
            body: JSON.stringify(payload),
            signal: window.currentChatController.signal
        });

        if (!response.ok) {
            throw new Error(`Server error: ${response.status} ${response.statusText}`);
        }

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        fullContent = '';
        let sseBuffer = '';

        while (true) {
            const { value, done } = await reader.read();
            if (done) break;

            sseBuffer += decoder.decode(value, { stream: true });
            const events = sseBuffer.split('\n');
            sseBuffer = events.pop() || '';

            for (const line of events) {
                const trimmed = line.trim();
                if (!trimmed || !trimmed.startsWith('data:')) continue;
                const data = trimmed.slice(5).trim();
                if (data === '[DONE]') continue;

                try {
                    const parsed = JSON.parse(data);
                    if (!parsed.choices || parsed.choices.length === 0) {
                        if (parsed.usage && (parsed.usage.tokens_per_second !== undefined || parsed.usage.model_header_info !== undefined)) {
                            if (hasStarted) {
                                renderTelemetry(aiMsgEl, parsed.usage, fullContent);
                            }
                        }
                        continue;
                    }
                    const delta = parsed.choices[0]?.delta;
                    if (!delta) continue;

                    if (!hasStarted) {
                        hasStarted = true;
                        updateStatus('User SMS Received');
                    }

                    // Handle Industry Standard tool_calls JSON array
                    if (delta.tool_calls && delta.tool_calls.length > 0) {
                        const toolsContainer = aiMsgEl.querySelector('.tools-container');
                        for (const call of delta.tool_calls) {
                            const callId = call.id || `call_${call.index}`;
                            let toolBlock = toolsContainer.querySelector(`#tool-${callId}`);
                            
                            if (!toolBlock) {
                                toolBlock = document.createElement('details');
                                toolBlock.id = `tool-${callId}`;
                                toolBlock.className = 'tool-accordion';
                                toolBlock.open = true;
                                toolBlock.style = "background: rgba(255,255,255,0.02); border: 1px solid rgba(255,255,255,0.1); border-radius: 8px; font-family: monospace; font-size: 0.85rem;";
                                
                                toolBlock.innerHTML = `
                                    <summary style="padding: 10px; cursor: pointer; display: flex; align-items: center; gap: 8px; font-weight: 500;">
                                        <svg class="tool-icon-spin" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="animation: spin 1s linear infinite;"><path d="M21 12a9 9 0 1 1-6.219-8.56"></path></svg>
                                        <span>Tool Call: <span style="color: #60a5fa;" class="tool-name-span">${escapeHtml(call.function?.name || 'Unknown')}</span></span>
                                    </summary>
                                    <div style="padding: 10px; border-top: 1px solid rgba(255,255,255,0.05);">
                                        <strong>Request Payload:</strong>
                                        <pre style="background: rgba(0,0,0,0.3); padding: 8px; border-radius: 4px; border: 1px solid rgba(255,255,255,0.05); margin-top: 5px;"><code class="tool-args-code">${escapeHtml(call.function?.arguments || '')}</code></pre>
                                        <div class="tool-result-container" style="margin-top: 10px; border-top: 1px solid rgba(255,255,255,0.1); padding-top: 10px; color: #9ca3af; font-style: italic;">Executing in Sandbox...</div>
                                    </div>
                                `;
                                toolsContainer.appendChild(toolBlock);
                            } else {
                                if (call.function?.arguments) {
                                    const codeEl = toolBlock.querySelector('.tool-args-code');
                                    codeEl.textContent += call.function.arguments;
                                }
                            }
                        }
                        updateStatus('Tool Call emitted from LLM.');
                        continue;
                    }

                    // Handle Custom Result Completion Event
                    if (delta.cluaiz_tool_result) {
                        const callId = delta.cluaiz_tool_result.id;
                        const resultText = delta.cluaiz_tool_result.result;
                        const toolsContainer = aiMsgEl.querySelector('.tools-container');
                        const toolBlock = toolsContainer.querySelector(`#tool-${callId}`);
                        if (toolBlock) {
                            const iconEl = toolBlock.querySelector('.tool-icon-spin');
                            if (iconEl) {
                                iconEl.outerHTML = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#4ade80" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>`;
                            }
                            const resContainer = toolBlock.querySelector('.tool-result-container');
                            resContainer.style.color = '#a7f3d0';
                            resContainer.style.fontStyle = 'normal';
                            resContainer.innerHTML = `<strong>Result:</strong><br/><pre style="white-space: pre-wrap; margin: 0; background: transparent; padding: 0;">${escapeHtml(resultText)}</pre>`;
                        }
                        updateStatus(`Sandbox executed tool successfully.`);
                        continue;
                    }

                    if (!delta.content) continue;
                    const content = delta.content;

                    if (statusContainer.style.display !== 'none') {
                        statusContainer.style.display = 'none';
                        divider.style.display = 'none';
                    }

                    fullContent += content;

                    // Thinking detection
                    let justFinishedThinking = false;
                    if (fullContent.includes('<think>') && !isThinking) {
                        isThinking = true;
                        window.dispatchEvent(new CustomEvent('chat:thinking_start'));
                    }
                    if (fullContent.includes('</think>') && isThinking) {
                        isThinking = false;
                        justFinishedThinking = true;
                        window.dispatchEvent(new CustomEvent('chat:thinking_end'));
                    }

                    // Render content with optional skip filtering
                    let displayContent = fullContent;
                    if (skipThinking) {
                        displayContent = fullContent.replace(/<think>[\s\S]*?(<\/think>|$)/g, '');
                    } else {
                        displayContent = fullContent.replace(/<think>([\s\S]*?)(<\/think>|$)/g, (match, content, endTag) => {
                            const isOpen = endTag === '' ? 'open' : '';
                            const summaryText = endTag === '' ? 'Thinking Process...' : 'Thought Process';
                            return `\n\n<details class="think-accordion" ${isOpen}><summary><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="think-icon"><polyline points="6 9 12 15 18 9"></polyline></svg> <span>${summaryText}</span></summary><div class="think-content">\n\n${content}\n\n</div></details>\n\n`;
                        });
                    }

                    // Only auto-scroll if user is currently near the bottom
                    const isNearBottom = container.scrollHeight - container.scrollTop - container.clientHeight < 100;
                    
                    const activeStates = [];
                    if (!skipThinking) {
                        aiTextEl.querySelectorAll('details.think-accordion').forEach(d => {
                            activeStates.push(d.open);
                        });
                    }

                    if (typeof marked !== 'undefined') {
                        if (!window.cluaizMarkedRenderer) {
                            window.cluaizMarkedRenderer = new marked.Renderer();
                            window.cluaizMarkedRenderer.code = function(code, language) {
                                language = language || 'plaintext';
                                const escapedCode = code.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
                                // Use base64 encoding to safely pass the code in the onclick attribute
                                const encodedData = btoa(encodeURIComponent(code));
                                return '<div class="code-block-wrapper" style="position: relative; margin-top: 1em; margin-bottom: 1em; background: #1e1e1e; border-radius: 6px; overflow: hidden; border: 1px solid rgba(255,255,255,0.1);">' +
                                    '<div style="background: rgba(255,255,255,0.05); padding: 6px 12px; display: flex; justify-content: space-between; align-items: center; color: #9ca3af; font-family: monospace; font-size: 0.75rem; border-bottom: 1px solid rgba(255,255,255,0.05);">' +
                                        '<span>' + escapeHtml(language) + '</span>' +
                                        '<button class="copy-code-btn" style="background: transparent; border: none; color: #9ca3af; cursor: pointer; display: flex; align-items: center; gap: 4px; font-size: 0.75rem; transition: color 0.2s;" onclick="window.copyCodeBlock(this, \'' + encodedData + '\')">' +
                                            '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg> Copy' +
                                        '</button>' +
                                    '</div>' +
                                    '<pre style="margin: 0; padding: 12px; overflow-x: auto; font-size: 0.85rem;"><code class="language-' + escapeHtml(language) + '">' + escapedCode + '</code></pre>' +
                                '</div>';
                            };
                        }
                        aiTextEl.innerHTML = marked.parse(displayContent, { renderer: window.cluaizMarkedRenderer });
                    } else {
                        aiTextEl.innerHTML = displayContent; // Use innerHTML to render details if marked is missing
                    }

                    if (!skipThinking) {
                        const newDetails = aiTextEl.querySelectorAll('details.think-accordion');
                        newDetails.forEach((d, i) => {
                            if (justFinishedThinking && i === newDetails.length - 1) {
                                d.open = false; // Close automatically when thinking ends
                            } else if (i < activeStates.length) {
                                d.open = activeStates[i]; // Restore user's manual toggle state
                            }
                        });
                    }
                    
                    if (isNearBottom) {
                        container.scrollTop = container.scrollHeight;
                    }

                } catch (_parseErr) { }
            }
        }

        if (fullContent.trim()) {
            conversationHistory.push({ role: 'assistant', content: fullContent });
            window.canContinue = true;
        } else if (!fullContent) {
            aiTextEl.textContent = 'Error: No final response synthesized.';
        }

    } catch (e) {
        if (e.name === 'AbortError') {
            updateStatus('[Generation Paused]');
            isAborted = true;
            // Save what we have so far so continuing works seamlessly
            if (fullContent.trim() && !window.canContinue) {
                // Determine full content based on current DOM to ensure we don't save garbage
                // Actually we should just save the raw fullContent generated up to the abort point
                if (fullContent.trim()) {
                    conversationHistory.push({ role: 'assistant', content: fullContent });
                    window.canContinue = true;
                }
            }
        } else {
            updateStatus(`[Error] Connection failed: ${e.message}`);
            aiTextEl.textContent = 'Connection error: ' + e.message;
            showErrorTooltip('Connection error: ' + e.message);
        }
    } finally {
        if (statusContainer) {
            statusContainer.style.display = 'none';
        }
        if (divider) {
            divider.style.display = 'none';
        }
        window.removeEventListener('chat:skip_thinking', onSkipThinking);
        window.removeEventListener('chat:abort', onAbort);
        if (isAborted) {
            window.dispatchEvent(new CustomEvent('chat:aborted'));
        } else {
            window.dispatchEvent(new CustomEvent('chat:complete'));
        }
    }

    container.scrollTop = container.scrollHeight;
}

function appendMessage(content, role, isTyping = false) {
    const container = document.getElementById('chat-stream-container');
    const msgEl = document.createElement('div');
    msgEl.className = `chat-message ${role}-message`;

    const bubbleClass = role === 'user' ? 'user-bubble' : 'ai-bubble';

    if (isTyping) {
        msgEl.innerHTML = `
            <div class="message-bubble ${bubbleClass}">
                <p style="opacity: 0.5; font-style: italic;">${escapeHtml(content)}</p>
            </div>
        `;
    } else {
        msgEl.innerHTML = `
            <div class="message-bubble ${bubbleClass}">
                <p>${escapeHtml(content)}</p>
            </div>
        `;
    }

    container.appendChild(msgEl);
    return msgEl;
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function renderTelemetry(container, usage, fullContent) {
    let telemetryEl = container.querySelector('.telemetry-badge');
    if (!telemetryEl) {
        telemetryEl = document.createElement('div');
        telemetryEl.className = 'telemetry-badge';
        telemetryEl.style.cssText = 'margin-top: 12px; font-family: monospace; font-size: 0.75rem; color: #ffffff; display: flex; gap: 16px; border-top: 1px solid rgba(255, 255, 255, 0.15); padding-top: 8px; flex-wrap: wrap; align-items: center; position: relative;';
        container.querySelector('.message-bubble').appendChild(telemetryEl);
    }
    const tps = typeof usage.tokens_per_second === 'number' ? usage.tokens_per_second.toFixed(2) : '0.00';
    const time = typeof usage.total_time_ms === 'number' ? (usage.total_time_ms / 1000).toFixed(2) : '0.00';
    const ttft = typeof usage.time_to_first_token_ms === 'number' ? (usage.time_to_first_token_ms / 1000).toFixed(2) : '0.00';
    const tokens = usage.total_tokens || 0;
    
    let hardwareHtml = '';
    if (usage.hardware_snapshot && usage.hardware_snapshot.system_control) {
        let sc = usage.hardware_snapshot.system_control;
        let vram = sc.silicon_truth && sc.silicon_truth.accelerators && sc.silicon_truth.accelerators.gpus && sc.silicon_truth.accelerators.gpus.length > 0 ? (sc.silicon_truth.accelerators.gpus[0].vram_available_gb || 0).toFixed(1) : '?';
        hardwareHtml = `<span>💻 VRAM: ${vram} GB</span>`;
    }

    telemetryEl.innerHTML = `<span>⚡ ${tps} TPS</span><span>⏱️ ${time}s</span><span>🚀 ${ttft}s TTFT</span><span>🪙 ${tokens} Tokens</span>${hardwareHtml}`;

    // Add copy button here
    if (fullContent) {
        const copyBtn = document.createElement('button');
        copyBtn.title = "Copy text";
        copyBtn.style.cssText = "background: transparent; border: none; cursor: pointer; color: #9ca3af; display: flex; align-items: center; padding: 4px; border-radius: 4px; transition: color 0.2s; margin-left: auto;";
        copyBtn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>';
        
        copyBtn.addEventListener('click', () => {
            navigator.clipboard.writeText(fullContent).then(() => {
                const originalSvg = copyBtn.innerHTML;
                copyBtn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#4ade80" stroke-width="2"><polyline points="20 6 9 17 4 12"></polyline></svg>';
                setTimeout(() => copyBtn.innerHTML = originalSvg, 2000);
            });
        });
        telemetryEl.appendChild(copyBtn);
    }
}

function showErrorTooltip(message) {
    let tooltip = document.getElementById('chat-error-tooltip');
    if (!tooltip) {
        tooltip = document.createElement('div');
        tooltip.id = 'chat-error-tooltip';
        tooltip.style.cssText = 'position: fixed; top: 20px; right: 20px; background-color: #ef4444; color: white; padding: 12px 24px; border-radius: 8px; font-family: sans-serif; font-size: 14px; z-index: 9999; box-shadow: 0 4px 12px rgba(0,0,0,0.15); transition: opacity 0.3s ease-in-out; font-weight: 500;';
        document.body.appendChild(tooltip);
    }
    tooltip.textContent = message;
    tooltip.style.opacity = '1';
    tooltip.style.display = 'block';
    
    setTimeout(() => {
        tooltip.style.opacity = '0';
        setTimeout(() => tooltip.style.display = 'none', 300);
    }, 5000);
}
