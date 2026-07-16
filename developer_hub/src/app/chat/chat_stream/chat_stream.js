// Conversation history for context
const conversationHistory = [];

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
    if (!window.chatSendHandler) {
        window.chatSendHandler = (e) => {
            const container = document.getElementById('chat-stream-container');
            const content = e.detail?.message;
            if (!content || !container) return;

            // Activate the stream container (show it)
            if (!container.classList.contains('active')) {
                container.classList.add('active');

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
            sendToAI(content);
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
                        const pRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
                        if (pRes.ok) {
                            const pData = await pRes.json();
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
                                document.getElementById('info-context-size').textContent = `Context: ${proc.context_size || '?'} Allocated / ${proc.original_context || '?'} Original`;
                                document.getElementById('info-vram-usage').textContent = `VRAM: ${proc.vram_gb !== undefined ? proc.vram_gb.toFixed(2) + ' GB' : '?'}`;
                                document.getElementById('info-engine').textContent = `Engine: ${proc.engine || '?'}`;
                            } else {
                                const selectedModel = typeof getSelectedModel === 'function' ? getSelectedModel() : 'default';
                                document.getElementById('info-model-id').textContent = `Model: ${selectedModel && selectedModel !== 'default' ? selectedModel : 'None loaded'}`;
                                document.getElementById('info-context-size').textContent = 'Context: -';
                                document.getElementById('info-vram-usage').textContent = 'VRAM: -';
                                document.getElementById('info-engine').textContent = 'Engine: Idle (Ready)';
                            }
                        } else {
                            document.getElementById('info-model-id').textContent = 'Error: Auth Failed';
                            document.getElementById('info-context-size').textContent = 'Context: -';
                            document.getElementById('info-vram-usage').textContent = 'VRAM: -';
                            document.getElementById('info-engine').textContent = 'Engine: -';
                        }
                    } catch (err) {
                        console.error('Failed to fetch system ps:', err);
                        document.getElementById('info-model-id').textContent = 'Error: API Offline';
                        document.getElementById('info-context-size').textContent = 'Context: -';
                        document.getElementById('info-vram-usage').textContent = 'VRAM: -';
                        document.getElementById('info-engine').textContent = 'Engine: -';
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
                let mdContent = "# Chat Export\\n\\n";
                for (let msg of conversationHistory) {
                    const role = msg.role === 'user' ? 'User' : 'Cluaiz Engine';
                    mdContent += `### ${role}\\n${msg.content}\\n\\n---\\n\\n`;
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
    }
}

async function sendToAI(userMessage) {
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

        const response = await fetch(window.getApiBaseUrl() + '/v1/chat/completions', {
            method: 'POST',
            headers: headers,
            body: JSON.stringify({
                model: model,
                messages: conversationHistory,
                stream: true
            }),
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
                        if (parsed.usage && parsed.usage.tokens_per_second !== undefined) {
                            renderTelemetry(aiMsgEl, parsed.usage, fullContent);
                        }
                        continue;
                    }
                    const delta = parsed.choices[0]?.delta;
                    if (!delta || !delta.content) continue;

                    if (!hasStarted) {
                        hasStarted = true;
                        updateStatus('User SMS Received');
                    }

                    const content = delta.content;

                    // Intercept Engine Status Markers
                    if (content.startsWith('__STEP_2')) {
                        updateStatus(`Match Found -> ${content.split(':')[1] || 'Tool'}`);
                        continue;
                    }
                    if (content.startsWith('__STEP_3')) {
                        updateStatus('Dynamic JIT Layer rules compile & inject successfully.');
                        continue;
                    }
                    if (content.startsWith('__STEP_4')) {
                        updateStatus('Inference system parses user SMS input context.');
                        continue;
                    }
                    if (content.includes('<TRIGGER:') && content.includes('</TRIGGER>')) {
                        const toolMatch = content.match(/<TRIGGER:([^>]+)>/);
                        const toolName = (toolMatch ? toolMatch[1] : 'tool').split(':').pop();
                        updateStatus(`Match tag emitted -> <TRIGGER:${toolName}>`);
                        await new Promise(r => setTimeout(r, 300));
                        updateStatus(`Engine intercept triggered. Autoregressive loop PAUSED.`);
                        continue;
                    }
                    if (content.startsWith('__ENGINE_PAUSE_EXECUTE__')) {
                        const toolName = content.split(':')[1] || 'Tool';
                        updateStatus(`Sandbox UnifiedExecutor executed: '${toolName}'.`);
                        await new Promise(r => setTimeout(r, 300));
                        updateStatus(`KV-Cache parameters injected. Resuming loop...`);
                        await new Promise(r => setTimeout(r, 200));
                        continue;
                    }

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
                        aiTextEl.innerHTML = marked.parse(displayContent);
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
            // User aborted via Stop/Pause button
            const displayContent = skipThinking ? fullContent.replace(/<think>[\s\S]*?(<\/think>|$)/g, '') : (fullContent + '\\n\n*[Generation Paused]*');
            if (typeof marked !== 'undefined') {
                aiTextEl.innerHTML = marked.parse(displayContent);
            } else {
                aiTextEl.textContent = displayContent;
            }

            // Save what we have so far so continuing works seamlessly
            const finalHtmlText = aiTextEl.textContent.replace('[Generation Paused]', '').trim();
            if (finalHtmlText) {
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
        window.dispatchEvent(new CustomEvent('chat:complete'));
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
