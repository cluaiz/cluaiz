// Conversation history for context
const conversationHistory = [];

export async function mountChatStream(rootElement) {
    try {
        const response = await fetch('/src/app/chat/chat_stream/chat_stream.html?v=' + new Date().getTime());
        const html = await response.text();
        rootElement.innerHTML = html;

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

                // Hide dashboard content
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
            <p class="final-text"></p>
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
        const response = await fetch('http://localhost:8000/v1/chat/completions', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
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
        let fullContent = '';
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
                    if (!parsed.choices || parsed.choices.length === 0) continue;

                    const delta = parsed.choices[0]?.delta;
                    if (!delta || !delta.content) continue;

                    if (!hasStarted) {
                        hasStarted = true;
                        updateStatus('[Step 1] User SMS Received');
                    }

                    const content = delta.content;

                    // Intercept Engine Status Markers
                    if (content.startsWith('__STEP_2')) {
                        updateStatus(`[Step 2] Match Found -> ${content.split(':')[1] || 'Tool'}`);
                        continue;
                    }
                    if (content.startsWith('__STEP_3')) {
                        updateStatus('[Step 3] Dynamic JIT Layer rules compile & inject successfully.');
                        continue;
                    }
                    if (content.startsWith('__STEP_4')) {
                        updateStatus('[Step 4] Inference system parses user SMS input context.');
                        continue;
                    }
                    if (content.includes('<TRIGGER:') && content.includes('</TRIGGER>')) {
                        const toolMatch = content.match(/<TRIGGER:([^>]+)>/);
                        const toolName = (toolMatch ? toolMatch[1] : 'tool').split(':').pop();
                        updateStatus(`[Step 5] Match tag emitted -> <TRIGGER:${toolName}>`);
                        await new Promise(r => setTimeout(r, 300));
                        updateStatus(`[Step 7] Engine intercept triggered. Autoregressive loop PAUSED.`);
                        continue;
                    }
                    if (content.startsWith('__ENGINE_PAUSE_EXECUTE__')) {
                        const toolName = content.split(':')[1] || 'Tool';
                        updateStatus(`[Step 8] Sandbox UnifiedExecutor executed: '${toolName}'.`);
                        await new Promise(r => setTimeout(r, 300));
                        updateStatus(`[Step 9] KV-Cache parameters injected. Resuming loop...`);
                        await new Promise(r => setTimeout(r, 200));
                        continue;
                    }

                    if (statusContainer.style.display !== 'none') {
                        statusContainer.style.display = 'none';
                        divider.style.display = 'none';
                    }

                    fullContent += content;

                    // Thinking detection
                    if (fullContent.includes('<think>') && !isThinking) {
                        isThinking = true;
                        window.dispatchEvent(new CustomEvent('chat:thinking_start'));
                    }
                    if (fullContent.includes('</think>') && isThinking) {
                        isThinking = false;
                        window.dispatchEvent(new CustomEvent('chat:thinking_end'));
                    }

                    // Render content with optional skip filtering
                    let displayContent = fullContent;
                    if (skipThinking) {
                        displayContent = fullContent.replace(/<think>[\s\S]*?(<\/think>|$)/g, '');
                    }
                    aiTextEl.textContent = displayContent;
                    container.scrollTop = container.scrollHeight;

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
            const displayContent = skipThinking ? aiTextEl.textContent : (aiTextEl.textContent + '\\n[Generation Paused]');
            aiTextEl.textContent = displayContent;

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
            addStatus(`[Error] Connection failed: ${e.message}`);
            aiTextEl.textContent = 'Connection error: ' + e.message;
        }
    } finally {
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

