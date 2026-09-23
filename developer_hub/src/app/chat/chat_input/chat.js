import { setupMicVoiceInput, checkAudioModelAndToggleMic } from './voice_input.js';

export async function mountChat(rootElement) {
    try {
        const response = await fetch('/src/app/chat/chat_input/chat.html?v=' + new Date().getTime());
        const html = await response.text();
        rootElement.innerHTML = html;

        if (!document.getElementById('chat-style')) {
            const link = document.createElement('link');
            link.id = 'chat-style';
            link.rel = 'stylesheet';
            link.href = '/src/app/chat/chat_input/chat.css?v=' + new Date().getTime();
            document.head.appendChild(link);
        }

        setTimeout(() => {
            if (window.lucide) {
                window.lucide.createIcons();
            }
            setupChatLogic();
        }, 100);

    } catch (e) {
        rootElement.innerHTML = `<h2 style="color:red; padding: 20px;">Failed to load chat: ${e.message}</h2>`;
    }
}

function setupChatLogic() {
    const textarea = document.getElementById('chat-textarea');
    const sendBtn = document.getElementById('send-btn');
    const sendWrapper = document.getElementById('send-wrapper');
    const attachBtn = document.getElementById('attach-btn');
    const attachMenu = document.getElementById('attach-menu');
    const attachWrapper = document.getElementById('attach-wrapper');

    const modelWrapper = document.getElementById('model-wrapper');
    const modelSelectBtn = document.getElementById('model-select-btn');
    const modelMenu = document.getElementById('model-menu');
    const selectedModelText = document.getElementById('selected-model-text');

    const micWrapper = document.getElementById('mic-wrapper');
    const skillsContainer = document.getElementById('skills-selected-container');

    const inputWrapper = document.getElementById('main-input-wrapper');
    const chatInputContainer = document.getElementById('chat-input-container');
    const bottomToolbar = document.getElementById('bottom-toolbar');

    const leftActionsContainer = document.getElementById('left-actions-container');
    const rightActionsContainer = document.getElementById('right-actions-container');
    const bottomLeftPlaceholder = document.getElementById('bottom-left-placeholder');
    const bottomRightPlaceholder = document.getElementById('bottom-right-placeholder');

    let isExpanded = false;
    let isThinkModeOn = false;
    let isGenerating = false;
    let isStopped = false;
    let wrapThreshold = Number.MAX_SAFE_INTEGER;
    const selectedSkills = new Set();

    // Dynamically fetch and populate models from backend
    fetchAndPopulateModels(modelMenu, selectedModelText, modelSelectBtn);

    // Dynamically fetch and populate tools/skills/plugins/extensions/mcp
    fetchAndPopulateTools(selectedSkills, updateSkillMenuVisuals, renderSkills);

    // Initial probe of context telemetry on chat mount
    setTimeout(() => {
        if (window.refreshContextTelemetry) window.refreshContextTelemetry();
    }, 400);

    // Setup Mic Voice Input Logic
    setupMicVoiceInput(textarea);

    // Textarea Auto-expand & Layout shift
    textarea.addEventListener('input', () => {
        // Handle Send Button visibility (Smooth transition)
        const hasText = textarea.value.trim().length > 0;

        if (hasText || isGenerating || isStopped) {
            sendWrapper.classList.remove('w-0', 'opacity-0', 'scale-0');
            sendWrapper.classList.add('w-[2.25rem]', 'opacity-100', 'scale-100');
        } else {
            sendWrapper.classList.add('w-0', 'opacity-0', 'scale-0');
            sendWrapper.classList.remove('w-[2.25rem]', 'opacity-100', 'scale-100');
        }

        // Update button icon based on state
        if (sendBtn) {
            if (hasText) {
                // Show Send icon (Paper plane) ALWAYS if there is text
                sendBtn.innerHTML = `
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <line x1="22" y1="2" x2="11" y2="13"></line>
                        <polygon points="22 2 15 22 11 13 2 9 22 2"></polygon>
                    </svg>
                `;
                sendBtn.classList.remove('text-primary', 'text-red-500', 'bg-transparent', 'bg-secondary');
                sendBtn.classList.add('bg-accent', 'text-black');
            } else if (isGenerating) {
                // Show Stop icon (Square)
                sendBtn.innerHTML = `
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <rect x="6" y="6" width="12" height="12"></rect>
                    </svg>
                `;
                sendBtn.classList.add('text-primary', 'bg-secondary');
                sendBtn.classList.remove('bg-accent', 'text-black', 'bg-transparent');
            } else if (isStopped) {
                // Show Cancel icon (Red X)
                sendBtn.innerHTML = `
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#ef4444" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                        <line x1="18" y1="6" x2="6" y2="18"></line>
                        <line x1="6" y1="6" x2="18" y2="18"></line>
                    </svg>
                `;
                sendBtn.classList.add('bg-secondary');
                sendBtn.classList.remove('text-primary', 'bg-accent', 'text-black', 'bg-transparent');
            }
        }


        // Handle Expansion
        textarea.style.height = 'auto'; // Reset
        let newHeight = textarea.scrollHeight;

        // Max height constraint
        if (newHeight > 144) {
            newHeight = 144;
            textarea.style.overflowY = 'auto';
        } else {
            textarea.style.overflowY = 'hidden';
        }
        textarea.style.height = newHeight + 'px';

        const hasNewline = textarea.value.includes('\n');

        let shouldExpand = false;
        let shouldCollapse = false;

        if (!isExpanded) {
            if (newHeight > 28 || hasNewline) {
                shouldExpand = true;
                if (hasNewline) {
                    wrapThreshold = Number.MAX_SAFE_INTEGER;
                } else {
                    wrapThreshold = textarea.value.length - 2;
                }
            }
        } else {
            if (!hasNewline && textarea.value.length < wrapThreshold && wrapThreshold !== Number.MAX_SAFE_INTEGER) {
                shouldCollapse = true;
                wrapThreshold = Number.MAX_SAFE_INTEGER;
            }
            if (!textarea.value) {
                shouldCollapse = true;
                wrapThreshold = Number.MAX_SAFE_INTEGER;
            }
        }

        if (shouldExpand) {
            isExpanded = true;
            inputWrapper.style.marginBottom = '24px';
            chatInputContainer.classList.remove('p-2');
            chatInputContainer.classList.add('p-3');
            textarea.classList.add('self-stretch');

            // Move buttons to bottom toolbar
            bottomToolbar.classList.remove('hidden');
            bottomToolbar.classList.add('flex');

            // Move containers
            bottomLeftPlaceholder.appendChild(attachWrapper);
            bottomRightPlaceholder.appendChild(skillsContainer);
            bottomRightPlaceholder.appendChild(modelWrapper);
            bottomRightPlaceholder.appendChild(micWrapper);
            bottomRightPlaceholder.appendChild(sendWrapper);

        } else if (shouldCollapse) {
            isExpanded = false;
            inputWrapper.style.marginBottom = '0px';
            chatInputContainer.classList.remove('p-3');
            chatInputContainer.classList.add('p-2');
            textarea.classList.remove('self-stretch');

            // Move buttons back to top row
            leftActionsContainer.appendChild(attachWrapper);

            rightActionsContainer.appendChild(skillsContainer);
            rightActionsContainer.appendChild(modelWrapper);
            rightActionsContainer.appendChild(micWrapper);
            rightActionsContainer.appendChild(sendWrapper);

            bottomToolbar.classList.remove('flex');
            bottomToolbar.classList.add('hidden');
        }
    });

    // Listen for generation state changes
    window.addEventListener('chat:start', () => {
        isGenerating = true;
        isStopped = false;
        if (sendBtn) {
            sendBtn.innerHTML = `
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="6" y="6" width="12" height="12"></rect>
                </svg>
            `;
            sendBtn.classList.add('text-primary', 'bg-secondary');
            sendBtn.classList.remove('bg-accent', 'text-black', 'bg-transparent');
        }
        textarea.dispatchEvent(new Event('input'));
    });

    window.addEventListener('chat:aborted', () => {
        isGenerating = false;
        isStopped = true;
        textarea.dispatchEvent(new Event('input'));
    });

    window.addEventListener('chat:complete', () => {
        isGenerating = false;
        isStopped = false;
        if (sendBtn) {
            sendBtn.innerHTML = `
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <line x1="22" y1="2" x2="11" y2="13"></line>
                    <polygon points="22 2 15 22 11 13 2 9 22 2"></polygon>
                </svg>
            `;
            sendBtn.classList.remove('text-primary', 'text-red-500', 'bg-transparent', 'bg-secondary');
            sendBtn.classList.add('bg-accent', 'text-black');
        }
        hideSkipThinking();
        textarea.dispatchEvent(new Event('input'));
    });

    textarea.addEventListener('keydown', function (e) {
        if (e.shiftKey && e.key === 'Enter') {
            if (isExpanded) return;
            e.preventDefault();
        } else if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            if (!isGenerating) {
                sendMessage();
            }
        }
    });

    if (sendBtn) {
        sendBtn.addEventListener('click', (e) => {
            e.preventDefault();
            const hasText = textarea.value.trim().length > 0;
            if (hasText) {
                if (isGenerating) {
                    window.dispatchEvent(new CustomEvent('chat:abort'));
                    isGenerating = false;
                    isStopped = false;
                }
                sendMessage();
            } else if (isGenerating) {
                window.dispatchEvent(new CustomEvent('chat:abort'));
            } else if (isStopped) {
                isStopped = false;
                window.dispatchEvent(new CustomEvent('chat:cancel'));
                textarea.dispatchEvent(new Event('input'));
            }
        });
    }

    function sendMessage() {
        if (isGenerating) return;
        const content = textarea.value.trim();
        // If content is empty but we have an assistant message at the end of history, we might be 'continuing'
        if (content.length > 0 || window.canContinue) {
            isStopped = false;

            let thinkModePayload = isThinkModeOn ? "on" : "auto";
            let overrideTemp = null;
            let systemConstraint = null;

            let selectedPredefined = null;
            ['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].forEach(s => {
                if (selectedSkills.has(s)) selectedPredefined = s;
            });

            if (selectedPredefined === "Think Deep") {
                thinkModePayload = "on";
                overrideTemp = 0.0;
                systemConstraint = "Analyze the request deeply step-by-step. Provide a highly detailed, comprehensive response.";
            } else if (selectedPredefined === "Think Lite") {
                thinkModePayload = "on";
                overrideTemp = 0.5;
                systemConstraint = "Think carefully but provide a balanced, concise response.";
            } else if (selectedPredefined === "Long Answer") {
                thinkModePayload = "off";
                overrideTemp = 0.7;
                systemConstraint = "Provide a detailed, thorough, and to-the-point answer.";
            } else if (selectedPredefined === "Short Answer") {
                thinkModePayload = "off";
                overrideTemp = 0.7;
                systemConstraint = "Provide a very concise, direct, and to-the-point answer.";
            }

            const activeTools = [];
            selectedSkills.forEach(s => {
                if (!['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].includes(s)) {
                    const turns = window.toolLifespans?.get(s) ?? 0;
                    activeTools.push({ name: s, turns: turns });
                }
            });

            window.dispatchEvent(new CustomEvent('chat:send', {
                detail: {
                    message: content,
                    think_mode: thinkModePayload,
                    temperature: overrideTemp,
                    system_prompt: systemConstraint,
                    tools: activeTools
                }
            }));
            textarea.value = '';
            textarea.dispatchEvent(new Event('input'));
        }
    }

    // Skip Thinking Button Logic
    const chatInputContainerEl = document.querySelector('.chat-input-container');
    const skipBtn = document.createElement('button');
    skipBtn.className = 'skip-thinking-btn hidden absolute -top-12 left-1/2 transform -translate-x-1/2 bg-secondary border border-border text-xs text-primary px-4 py-2 rounded-full shadow-lg hover:bg-hover transition-all z-50 flex items-center gap-2';
    skipBtn.innerHTML = `<span>⚡</span> Skip Thinking`;
    if (chatInputContainerEl && chatInputContainerEl.parentElement) {
        chatInputContainerEl.parentElement.style.position = 'relative';
        chatInputContainerEl.parentElement.appendChild(skipBtn);
    }

    skipBtn.addEventListener('click', () => {
        window.dispatchEvent(new CustomEvent('chat:skip_thinking'));
        hideSkipThinking();
    });

    window.addEventListener('chat:thinking_start', () => {
        skipBtn.classList.remove('hidden');
    });

    window.addEventListener('chat:thinking_end', () => {
        hideSkipThinking();
    });

    function hideSkipThinking() {
        skipBtn.classList.add('hidden');
    }

    // Submenu Logic
    function openSubmenu(menuId) {
        // Debounce to prevent flickering
        clearTimeout(window.submenuTimeout);
        window.submenuTimeout = setTimeout(() => {
            const allMenus = ['skills-menu', 'plugins-menu', 'mcp-menu', 'thinking-menu'];
            allMenus.forEach(id => {
                const el = document.getElementById(id);
                if (el) el.classList.add('hidden');
            });

            if (menuId) {
                const el = document.getElementById(menuId);
                if (el) el.classList.remove('hidden');
            }
        }, 150);
    }

    document.getElementById('skills-menu-wrapper')?.addEventListener('mouseenter', () => openSubmenu('skills-menu'));
    document.getElementById('plugins-menu-wrapper')?.addEventListener('mouseenter', () => openSubmenu('plugins-menu'));
    document.getElementById('mcp-menu-wrapper')?.addEventListener('mouseenter', () => openSubmenu('mcp-menu'));
    document.getElementById('thinking-menu-wrapper')?.addEventListener('mouseenter', () => openSubmenu('thinking-menu'));
    document.getElementById('upload-file-btn')?.addEventListener('mouseenter', () => openSubmenu(null));
    textarea.addEventListener('focus', () => {
        chatInputContainer.style.borderColor = 'var(--text-accent)';
    });
    textarea.addEventListener('blur', () => {
        chatInputContainer.style.borderColor = 'var(--border)';
    });
    attachMenu.addEventListener('mouseleave', () => openSubmenu(null));

    let isAttachOpen = false;
    attachBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        const icon = attachBtn.firstElementChild;
        isAttachOpen = !isAttachOpen;
        if (isAttachOpen) {
            attachMenu.classList.remove('hidden');
            attachMenu.classList.add('flex');
            if (icon) icon.classList.add('rotate-45');
            attachBtn.classList.add('text-accent', 'bg-secondary');
            attachBtn.classList.remove('text-muted');
        } else {
            attachMenu.classList.add('hidden');
            attachMenu.classList.remove('flex');
            if (icon) icon.classList.remove('rotate-45');
            attachBtn.classList.remove('text-accent', 'bg-secondary');
            attachBtn.classList.add('text-muted');
        }
    });

    // Model Menu Toggle
    let isModelOpen = false;
    modelSelectBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        isModelOpen = !isModelOpen;
        if (isModelOpen) {
            modelMenu.classList.remove('hidden');
            modelMenu.classList.add('flex');
            modelSelectBtn.classList.add('bg-secondary', 'text-primary');
            modelSelectBtn.classList.remove('text-muted');
        } else {
            modelMenu.classList.add('hidden');
            modelMenu.classList.remove('flex');
            modelSelectBtn.classList.remove('bg-secondary', 'text-primary');
            modelSelectBtn.classList.add('text-muted');
        }
    });

    // Close dropdowns on outside click
    document.addEventListener('click', (e) => {
        if (isAttachOpen && attachWrapper && !attachWrapper.contains(e.target)) {
            isAttachOpen = false;
            attachMenu.classList.add('hidden');
            attachMenu.classList.remove('flex');
            const icon = attachBtn.firstElementChild;
            if (icon) icon.classList.remove('rotate-45');
            attachBtn.classList.remove('text-accent', 'bg-secondary');
            attachBtn.classList.add('text-muted');
        }
        if (isModelOpen && modelWrapper && !modelWrapper.contains(e.target)) {
            isModelOpen = false;
            modelMenu.classList.add('hidden');
            modelMenu.classList.remove('flex');
            modelSelectBtn.classList.remove('bg-secondary', 'text-primary');
            modelSelectBtn.classList.add('text-muted');
        }
    });

    // Model Selection is handled dynamically in fetchAndPopulateModels()

    // Render Chips
    function renderSkills() {
        const container = document.getElementById('skills-selected-container');
        container.innerHTML = '';
        selectedSkills.forEach(skill => {
            const chip = document.createElement('div');
            chip.className = 'group flex-align gap-1 px-2 py-1 bg-transparent rounded-lg border-border-1px text-xs font-medium text-primary hover-border-muted transition-colors cursor-pointer';

            const formattedName = skill.split('-').map(word => word.charAt(0).toUpperCase() + word.slice(1)).join(' ');

            let iconStr = 'layers';
            let customSvgHtml = null;
            if (window.componentIcons && window.componentIcons.has(skill)) {
                const info = window.componentIcons.get(skill);
                iconStr = info.catIcon || iconStr;
                if (info.customSvg) {
                    customSvgHtml = `<div class="text-muted flex-center" style="width: 14px; height: 14px; display: inline-flex; align-items: center; justify-content: center;">
                                        ${info.customSvg.replace('<svg ', '<svg style="width:100%; height:100%;" ')}
                                     </div>`;
                }
            } else {
                if (skill === 'Web Search') iconStr = 'globe';
                else if (skill === 'Deep Research') iconStr = 'telescope';
                else if (skill === 'Think Deep') iconStr = 'brain';
                else if (skill === 'Think Lite') iconStr = 'zap';
                else if (skill === 'Long Answer') iconStr = 'align-justify';
                else if (skill === 'Short Answer') iconStr = 'align-left';
            }

            const defaultIconHtml = customSvgHtml ? customSvgHtml : `<i data-lucide="${iconStr}" class="w-3-5 h-3-5"></i>`;

            window.toolLifespans = window.toolLifespans || new Map();
            const currentTurns = window.toolLifespans.get(skill) ?? 0;
            const turnBadgeText = currentTurns === -1 ? '∞' : (currentTurns === 0 ? '1T' : `${currentTurns}T`);
            const turnBadgeTitle = currentTurns === -1 ? 'Persistent (all turns)' : (currentTurns === 0 ? 'Ephemeral (1 turn)' : `${currentTurns} Turns Countdown`);

            chip.innerHTML = `
                <div class="icon-default flex-center">
                    ${defaultIconHtml}
                </div>
                <div class="icon-hover flex-center" style="display: none;">
                    <i data-lucide="x" class="w-3-5 h-3-5 text-red-500"></i>
                </div>
                <span class="hidden sm:inline">${formattedName}</span>
                <span class="tool-turns-badge" title="${turnBadgeTitle} (Click to toggle)" style="font-size: 0.65rem; background: rgba(255,255,255,0.12); border-radius: 4px; padding: 1px 4px; margin-left: 2px; color: #a1a1aa; cursor: pointer; transition: background 0.2s ease;">${turnBadgeText}</span>
            `;

            const turnsBadge = chip.querySelector('.tool-turns-badge');
            if (turnsBadge) {
                turnsBadge.addEventListener('click', (e) => {
                    e.stopPropagation();
                    const cur = window.toolLifespans.get(skill) ?? 0;
                    let next = 0;
                    if (cur === 0) next = 3;
                    else if (cur === 3) next = -1;
                    else next = 0;
                    window.toolLifespans.set(skill, next);
                    renderSkills();
                });
            }

            chip.addEventListener('mouseenter', () => {
                chip.querySelector('.icon-default').style.display = 'none';
                chip.querySelector('.icon-hover').style.display = 'flex';
                chip.style.borderColor = 'rgba(239, 68, 68, 0.5)'; // red-500 border hint
            });
            chip.addEventListener('mouseleave', () => {
                chip.querySelector('.icon-default').style.display = 'flex';
                chip.querySelector('.icon-hover').style.display = 'none';
                chip.style.borderColor = ''; // reset
            });

            chip.addEventListener('click', (e) => {
                e.stopPropagation();
                selectedSkills.delete(skill);
                window.toolLifespans.delete(skill);
                renderSkills();
            });

            container.appendChild(chip);
        });

        const activeToolCount = Array.from(selectedSkills).filter(s => !['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].includes(s)).length;
        const badgeEl = document.getElementById('pc-tools-badge');
        if (badgeEl) badgeEl.textContent = activeToolCount;
        if (window.refreshContextTelemetry) {
            window.refreshContextTelemetry();
        }

        if (window.lucide) window.lucide.createIcons();
    }

    function updateSkillMenuVisuals() {
        document.querySelectorAll('.skill-btn').forEach(btn => {
            const skill = btn.getAttribute('data-skill');
            const isSelected = selectedSkills.has(skill);

            // Remove existing checkmarks/dots
            const existingCheck = btn.querySelector('.lucide-check');
            if (existingCheck) existingCheck.remove();
            const existingDot = btn.querySelector('.skill-dot');
            if (existingDot) existingDot.remove();

            if (['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].includes(skill)) {
                if (isSelected) {
                    btn.classList.add('bg-secondary', 'border-border-1px', 'shadow-sm', 'text-accent');
                    btn.classList.remove('bg-transparent', 'border-transparent', 'text-primary');
                    const check = document.createElement('i');
                    check.setAttribute('data-lucide', 'check');
                    check.className = 'w-3-5 h-3-5 flex-shrink-0';
                    btn.appendChild(check);
                } else {
                    btn.classList.remove('bg-secondary', 'border-border-1px', 'shadow-sm', 'text-accent');
                    btn.classList.add('bg-transparent', 'border-transparent', 'text-primary');
                }
            } else {
                if (isSelected) {
                    const check = document.createElement('i');
                    check.setAttribute('data-lucide', 'check');
                    check.className = 'w-3-5 h-3-5 flex-shrink-0 ml-auto text-accent';
                    btn.appendChild(check);
                }
            }
        });

        // Sync PC Tools checkboxes and switches
        document.querySelectorAll('.pc-tool-checkbox').forEach(cb => {
            const toolId = cb.getAttribute('data-id');
            const isChecked = selectedSkills.has(toolId);
            cb.checked = isChecked;
            const slider = cb.nextElementSibling;
            if (slider) {
                slider.style.background = isChecked ? '#22c55e' : '#3f3f46';
                const knob = slider.firstElementChild;
                if (knob) knob.style.left = isChecked ? '17px' : '2px';
            }
        });

        const activeCount = Array.from(selectedSkills).filter(s => !['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].includes(s)).length;
        const badgeEl = document.getElementById('pc-tools-badge');
        if (badgeEl) badgeEl.textContent = activeCount;

        // Render all the newly created icons
        if (typeof lucide !== 'undefined') {
            lucide.createIcons();
        }

        // Update main thinking menu button
        const mainThinkingBtn = document.getElementById('thinking-menu-btn');
        const mainThinkingContent = document.getElementById('main-thinking-content');
        if (mainThinkingBtn && mainThinkingContent) {
            let hasThinkSkill = false;
            let thinkIcon = 'zap';
            ['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].forEach(s => {
                if (selectedSkills.has(s)) {
                    hasThinkSkill = true;
                    if (s === 'Think Deep') thinkIcon = 'brain';
                    else if (s === 'Think Lite') thinkIcon = 'zap';
                    else if (s === 'Long Answer') thinkIcon = 'align-justify';
                    else if (s === 'Short Answer') thinkIcon = 'align-left';
                }
            });

            if (hasThinkSkill) {
                mainThinkingBtn.classList.add('text-accent');
                mainThinkingBtn.classList.remove('text-primary');
                mainThinkingContent.innerHTML = `<i data-lucide="${thinkIcon}" class="w-4 h-4 text-accent"></i><span>Thinking</span>`;
            } else {
                mainThinkingBtn.classList.remove('text-accent');
                mainThinkingBtn.classList.add('text-primary');
                mainThinkingContent.innerHTML = `<i data-lucide="zap" class="w-4 h-4 text-muted group-hover-accent"></i><span>Thinking</span>`;
            }
        }

        if (window.lucide) window.lucide.createIcons();
    }

    // Skill Selection
    document.querySelectorAll('.skill-btn').forEach(btn => {
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            const skill = btn.getAttribute('data-skill');

            if (['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].includes(skill)) {
                ['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].forEach(s => selectedSkills.delete(s));
            }

            if (selectedSkills.has(skill)) {
                selectedSkills.delete(skill);
            } else {
                selectedSkills.add(skill);
            }

            updateSkillMenuVisuals();
            renderSkills();
            attachMenu.classList.add('hidden');
            attachMenu.classList.remove('flex');
            isAttachOpen = false;

            const icon = attachBtn.firstElementChild;
            if (icon) icon.classList.remove('rotate-45');
            attachBtn.classList.remove('text-accent', 'bg-secondary');
            attachBtn.classList.add('text-muted');
        });
    });
    let isResponseLengthMapEnabled = true;

    async function fetchModelConfig() {
        try {
            // First determine if we are using GGUF or ONNX
            let configEndpoint = '/v1/system/gguf_config';
            try {
                const permRes = await fetch(window.getApiBaseUrl() + '/v1/system/permissions');
                if (permRes.ok) {
                    const permData = await permRes.json();
                    if (permData.chat_models && permData.chat_models.text) {
                        const activeModelId = permData.chat_models.text.toLowerCase();
                        if (activeModelId.endsWith('.onnx') || activeModelId.includes('-onnx')) {
                            configEndpoint = '/v1/system/onnx_config';
                        }
                    }
                }
            } catch (e) { }

            const res = await fetch(window.getApiBaseUrl() + configEndpoint);
            if (res.ok) {
                const data = await res.json();
                if (data.user_moved_flags) {
                    if (data.user_moved_flags.think_mode === "On") {
                        isThinkModeOn = true;
                        const thinkToggle = document.getElementById('think-toggle');
                        const thinkToggleThumb = document.getElementById('think-toggle-thumb');
                        if (thinkToggle && thinkToggleThumb) {
                            thinkToggle.classList.replace('bg-secondary', 'bg-accent');
                            thinkToggle.style.borderColor = 'transparent';
                            thinkToggleThumb.style.transform = 'translateY(-50%) translateX(14px)';
                        }
                    }

                    if (data.user_moved_flags.response_length) {
                        isResponseLengthMapEnabled = data.user_moved_flags.response_length['type'] !== 'custom';

                        const thinkOptionsOn = document.getElementById('think-options-on');
                        const thinkOptionsOff = document.getElementById('think-options-off');
                        const customResponseLength = document.getElementById('custom-response-length');

                        if (!isResponseLengthMapEnabled) {
                            if (thinkOptionsOn) thinkOptionsOn.style.display = 'none';
                            if (thinkOptionsOff) thinkOptionsOff.style.display = 'none';
                            if (customResponseLength) customResponseLength.style.display = 'flex';
                        }
                    }
                }
            }
        } catch (e) {
            console.error('Failed to fetch model config', e);
        }
    }
    fetchModelConfig();

    // Think Toggle
    const thinkToggle = document.getElementById('think-toggle');
    const thinkToggleThumb = document.getElementById('think-toggle-thumb');
    const thinkOptionsOn = document.getElementById('think-options-on');
    const thinkOptionsOff = document.getElementById('think-options-off');
    const customResponseLength = document.getElementById('custom-response-length');

    thinkToggle.addEventListener('click', async (e) => {
        e.stopPropagation();
        isThinkModeOn = !isThinkModeOn;
        const newThinkMode = isThinkModeOn ? "On" : "Off";

        if (isThinkModeOn) {
            thinkToggle.classList.replace('bg-secondary', 'bg-accent');
            thinkToggle.style.borderColor = 'transparent';
            thinkToggleThumb.style.transform = 'translateY(-50%) translateX(14px)';
        } else {
            thinkToggle.classList.replace('bg-accent', 'bg-secondary');
            thinkToggle.style.borderColor = 'var(--border-color)';
            thinkToggleThumb.style.transform = 'translateY(-50%) translateX(2px)';
        }

        // Real-time backend sync via standard gguf_config / onnx_config APIs
        try {
            const gRes = await fetch(window.getApiBaseUrl() + '/v1/system/gguf_config');
            if (gRes.ok) {
                const ggufData = await gRes.json();
                if (!ggufData.user_moved_flags) ggufData.user_moved_flags = {};
                ggufData.user_moved_flags.think_mode = newThinkMode;
                await fetch(window.getApiBaseUrl() + '/v1/system/gguf_config', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(ggufData)
                });
            }
            const oRes = await fetch(window.getApiBaseUrl() + '/v1/system/onnx_config');
            if (oRes.ok) {
                const onnxData = await oRes.json();
                if (!onnxData.user_moved_flags) onnxData.user_moved_flags = {};
                onnxData.user_moved_flags.think_mode = newThinkMode;
                await fetch(window.getApiBaseUrl() + '/v1/system/onnx_config', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(onnxData)
                });
            }
        } catch (err) {
            console.error('Failed to sync think_mode in real-time:', err);
        }

        if (!isResponseLengthMapEnabled) {
            // Custom mode ignores Think Toggle State
            if (thinkOptionsOn) thinkOptionsOn.style.display = 'none';
            if (thinkOptionsOff) thinkOptionsOff.style.display = 'none';
            if (customResponseLength) customResponseLength.style.display = 'flex';
        } else {
            // Predefined mode respects Think Toggle State
            if (customResponseLength) customResponseLength.style.display = 'none';
            if (isThinkModeOn) {
                if (thinkOptionsOn) thinkOptionsOn.style.display = 'flex';
                if (thinkOptionsOff) thinkOptionsOff.style.display = 'none';
            } else {
                if (thinkOptionsOn) thinkOptionsOn.style.display = 'none';
                if (thinkOptionsOff) thinkOptionsOff.style.display = 'flex';
            }
        }

        // clear thinking skills on toggle
        ['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].forEach(s => selectedSkills.delete(s));
        updateSkillMenuVisuals();
        renderSkills();
    });
}

// ─── Model Name Formatter (Ported from Tauri ChatInput.tsx) ─────────
function formatModelName(rawFilename) {
    if (!rawFilename) return { fullName: 'Unknown Model', shortName: 'Unknown' };

    const parts = rawFilename.split(':');

    const formatString = (str) => {
        let name = str.replace(/[-_]/g, ' ');
        return name.split(' ').map(word => {
            if (!word) return '';
            if (word.toLowerCase() === 'r1') return 'R1';
            if (word.match(/^[e]?\d+(\.\d+)?b$/i)) return word.toUpperCase();
            if (word.match(/^v\d+$/i)) return word.toUpperCase();
            return word.charAt(0).toUpperCase() + word.slice(1).toLowerCase();
        }).join(' ');
    };

    const shortName = formatString(parts[0] || 'Unknown');
    let fullName = shortName;

    if (parts.length > 1) {
        const paramStr = parts[1].toLowerCase();
        if (paramStr !== 'unknown' && paramStr !== 'gguf' && paramStr !== 'onnx' && !paramStr.match(/^[qf]\d+/)) {
            fullName = `${shortName} ${formatString(parts[1])}`;
        }
    }

    return { fullName, shortName: fullName };
}

// ─── Dynamic Model Fetching from /v1/models/installed ───────────────
async function fetchAndPopulateModels(modelMenu, selectedModelText, modelSelectBtn) {
    const menuInner = modelMenu.querySelector('#model-menu-inner') || modelMenu;

    try {
        const response = await fetch(window.getApiBaseUrl() + '/v1/models/installed');
        if (!response.ok) throw new Error(`HTTP ${response.status}`);

        const data = await response.json();
        const installedRaw = data.installed || data.installed_models || data.models || [];
        const installed = Array.isArray(installedRaw) ? installedRaw : Object.values(installedRaw);

        // Delegate audio model STT detection to modular voice_input component
        checkAudioModelAndToggleMic(installed);

        // Filter for chat models only (same logic as Tauri app)
        const chatModels = installed.filter(m => m.category === 'chat');

        if (chatModels.length === 0) {
            menuInner.innerHTML = `
                <div class="px-3 py-2-5 text-xs text-muted" style="text-align: center; opacity: 0.6;">
                    No models installed
                </div>
            `;
            selectedModelText.textContent = 'No Model';
            return;
        }

        // Fetch active model from permission.json
        let activeModelId = chatModels.length > 0 ? chatModels[0].id : 'default';
        try {
            const permRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
            if (permRes.ok) {
                const permData = await permRes.json();
                const perm = permData.permission || permData;
                const activeId = perm.active_slots?.chat_slot?.model_id || perm.chat_models?.text;
                if (activeId) {
                    activeModelId = activeId;
                }
            }
        } catch (e) {
            console.error('Failed to read active model:', e);
        }

        // 🛡️ Auto-heal: Ensure activeModelId is actually present in installed chatModels
        const normalizeId = (id) => id ? id.replace(/[-_:]/g, '').toLowerCase() : '';
        const foundInInstalled = chatModels.find(m => normalizeId(m.id) === normalizeId(activeModelId));
        if (foundInInstalled) {
            activeModelId = foundInInstalled.id;
        } else if (chatModels.length > 0) {
            console.warn(`[Chat] Configured model '${activeModelId}' is not installed. Auto-healing to '${chatModels[0].id}'.`);
            activeModelId = chatModels[0].id;
        }

        // Set the active model
        const activeFormatted = formatModelName(activeModelId);
        selectedModelText.textContent = activeFormatted.shortName;
        selectedModelText.dataset.modelId = activeModelId;
        window.currentActiveModelId = activeModelId;

        // Build dropdown buttons
        menuInner.innerHTML = '';
        chatModels.forEach((model, index) => {
            const formatted = formatModelName(model.id);
            const btn = document.createElement('button');
            btn.className = 'model-option w-full flex-between px-3 py-2-5 text-xs font-medium rounded-lg text-muted hover-bg-secondary hover-text-primary group';
            btn.setAttribute('data-model', model.id);

            // Relaxed equality check (Windows paths use dashes instead of colons for model IDs)
            const normalizeId = (id) => id.replace(/[-_:]/g, '').toLowerCase();
            const isActive = normalizeId(model.id) === normalizeId(activeModelId);

            btn.innerHTML = `
                <div class="flex-align gap-2 truncate">
                    <span class="truncate">${formatted.fullName}</span>
                </div>
                ${isActive ? '<i data-lucide="check" class="w-3-5 h-3-5 flex-shrink-0"></i>' : ''}
            `;

            // Click handler for model selection
            btn.addEventListener('click', async () => {
                selectedModelText.textContent = formatted.shortName;
                selectedModelText.dataset.modelId = model.id;
                window.currentActiveModelId = model.id;
                modelMenu.classList.add('hidden');
                modelMenu.classList.remove('flex');
                modelSelectBtn.classList.remove('bg-secondary', 'text-primary');
                modelSelectBtn.classList.add('text-muted');

                // Update checkmarks
                menuInner.querySelectorAll('.lucide-check').forEach(el => el.remove());
                const check = document.createElement('i');
                check.setAttribute('data-lucide', 'check');
                check.className = 'w-3-5 h-3-5 flex-shrink-0';
                btn.appendChild(check);
                if (window.lucide) window.lucide.createIcons();

                // Update active model in permission.json via API
                try {
                    const permRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
                    const permData = await permRes.json();
                    const newPerm = permData.permission || permData;
                    if (!newPerm.active_slots) newPerm.active_slots = {};
                    if (!newPerm.active_slots.chat_slot) newPerm.active_slots.chat_slot = {};
                    newPerm.active_slots.chat_slot.model_id = model.id;
                    if (!newPerm.chat_models) newPerm.chat_models = {};
                    newPerm.chat_models.text = model.id;

                    await fetch(window.getApiBaseUrl() + '/v1/system/permission', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(newPerm)
                    });

                    // 🎯 Live UI & Context Bar Real-Time Update for Newly Selected Model
                    const popoverModelMax = document.getElementById('popover-model-max');
                    const limitEl = document.getElementById('live-ctx-limit');
                    const usedEl = document.getElementById('live-ctx-used');
                    const pctEl = document.getElementById('live-ctx-pct');
                    const popoverTotal = document.getElementById('popover-ctx-total');
                    const freeSpaceEl = document.getElementById('item-free-space-val');
                    const freePctEl = document.getElementById('item-free-space-pct');

                    const ctxWindow = model.metadata?.context_window || model.context_length || model.context_window;
                    if (popoverModelMax) {
                        popoverModelMax.textContent = ctxWindow ? formatK(ctxWindow) : '—';
                    }

                    // Default to '—' (dash) until message is sent and backend Negotiator returns actual usable allocation
                    if (limitEl) limitEl.textContent = '—';
                    if (usedEl) usedEl.textContent = '0';
                    if (pctEl) pctEl.textContent = '0%';
                    if (popoverTotal) popoverTotal.textContent = '0 / — (0%)';
                    if (freeSpaceEl) freeSpaceEl.textContent = '—';
                    if (freePctEl) freePctEl.textContent = '0%';

                    const livePerfStats = document.getElementById('live-perf-stats');
                    if (livePerfStats) livePerfStats.style.display = 'none';

                    // Reset breakdown progress bars & values
                    ['bar-messages', 'bar-system-prompt', 'bar-skills', 'bar-plugins', 'bar-mcp-tools'].forEach(id => {
                        const el = document.getElementById(id);
                        if (el) el.style.width = '0%';
                    });
                    ['item-messages-val', 'item-system-prompt-val', 'item-skills-val', 'item-plugins-val', 'item-mcp-tools-val'].forEach(id => {
                        const el = document.getElementById(id);
                        if (el) el.textContent = '0';
                    });
                    ['item-messages-pct', 'item-system-prompt-pct', 'item-skills-pct', 'item-plugins-pct', 'item-mcp-tools-pct'].forEach(id => {
                        const el = document.getElementById(id);
                        if (el) el.textContent = '0%';
                    });

                    window.dispatchEvent(new CustomEvent('model-changed', { detail: { modelId: model.id, model } }));
                    if (window.refreshContextTelemetry) window.refreshContextTelemetry();
                } catch (e) {
                    console.error('Failed to update active model in permissions:', e);
                }
            });

            menuInner.appendChild(btn);
        });

        // Render lucide icons for the new buttons
        if (window.lucide) window.lucide.createIcons();

    } catch (e) {
        console.error('Failed to fetch installed models:', e);
        menuInner.innerHTML = `
            <div class="px-3 py-2-5 text-xs text-muted" style="text-align: center; opacity: 0.6;">
                Failed to load models
            </div>
        `;
        selectedModelText.textContent = 'No Model';
    }
}

// ─── Dynamic Tools/Skills Fetching ──────────────────────────────────────
async function fetchAndPopulateTools(selectedSkills, updateSkillMenuVisuals, renderSkills) {
    try {
        const response = await fetch('/api/components/list');
        if (!response.ok) throw new Error(`HTTP ${response.status}`);

        const data = await response.json();
        const richData = data.rich || {};

        const categories = [
            { key: 'skill', menuId: 'skills-menu', icon: 'layers', colorClass: 'hover-text-accent', badgeBg: 'rgba(236, 72, 153, 0.15)', badgeColor: '#f472b6' },
            { key: 'plugin', menuId: 'plugins-menu', icon: 'box', colorClass: 'hover-text-blue', badgeBg: 'rgba(59, 130, 246, 0.15)', badgeColor: '#93c5fd' },
            { key: 'mcp', menuId: 'mcp-menu', icon: 'server', colorClass: 'hover-text-emerald', badgeBg: 'rgba(16, 185, 129, 0.15)', badgeColor: '#6ee7b7' }
        ];

        window.componentIcons = window.componentIcons || new Map();
        const allToolEntries = [];

        for (const cat of categories) {
            const menu = document.getElementById(cat.menuId);
            const richItems = richData[cat.key] || [];
            const rawNames = data[cat.key] || [];

            // Combine into unified tool items list
            const toolItems = richItems.length > 0 ? richItems : rawNames.map(name => ({
                id: name,
                name: name,
                category: cat.key,
                enabled: true,
                security_mode: 'sandboxed',
                description: 'Host PC Capability'
            }));

            if (menu) {
                menu.innerHTML = '';
                if (toolItems.length === 0) {
                    const displayName = cat.key === 'mcp' ? 'MCPs' : cat.key.charAt(0).toUpperCase() + cat.key.slice(1) + 's';
                    menu.innerHTML = `<div class="p-2 text-xs text-muted text-center italic">No ${displayName} available</div>`;
                } else {
                    toolItems.forEach(item => {
                        allToolEntries.push(item);
                        const itemName = item.id;
                        const formattedName = item.name || itemName.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');

                        const btn = document.createElement('button');
                        btn.className = `dropdown-item w-full flex-between text-primary ${cat.colorClass} hover-bg-secondary group skill-btn`;
                        btn.setAttribute('data-skill', itemName);

                        window.componentIcons.set(itemName, { catIcon: cat.icon, customSvg: item.icon_svg });
                        if (item.name) {
                            window.componentIcons.set(item.name, { catIcon: cat.icon, customSvg: item.icon_svg });
                        }

                        let iconHtml = item.icon_svg
                            ? `<span class="flex-center text-muted" style="width: 16px; height: 16px; display: inline-flex; align-items: center; justify-content: center;">${item.icon_svg.replace('<svg ', '<svg style="width:100%; height:100%; max-width:16px; max-height:16px;" ')}</span>`
                            : `<i data-lucide="${cat.icon}" class="w-4 h-4 text-muted"></i>`;

                        btn.innerHTML = `
                            <div class="flex-align gap-2-5">
                                ${iconHtml}
                                <span>${formattedName}</span>
                            </div>
                        `;

                        btn.addEventListener('click', (e) => {
                            e.stopPropagation();
                            if (selectedSkills.has(itemName)) {
                                selectedSkills.delete(itemName);
                            } else {
                                selectedSkills.add(itemName);
                            }
                            updateSkillMenuVisuals();
                            renderSkills();
                            if (window.refreshContextTelemetry) window.refreshContextTelemetry();
                        });

                        menu.appendChild(btn);
                    });
                }
            } else {
                toolItems.forEach(item => allToolEntries.push(item));
            }
        }

        // Cache all registered tools for the context inspector
        window.allRegisteredTools = allToolEntries;

        if (window.lucide) window.lucide.createIcons();
        updateSkillMenuVisuals();
        if (window.refreshContextTelemetry) window.refreshContextTelemetry();
    } catch (e) {
        console.error("Failed to fetch tools/skills list:", e);
    }
}

window.refreshContextTelemetry = async function() {
    try {
        const modelEl = document.getElementById('selected-model-text');
        const model = (modelEl && modelEl.textContent && modelEl.textContent !== 'Unknown Model') ? modelEl.textContent.trim() : 'default';
        const activeTools = typeof selectedSkills !== 'undefined' ? Array.from(selectedSkills).filter(s => !['Think Deep', 'Think Lite', 'Long Answer', 'Short Answer'].includes(s)) : [];
        const sessionId = window.currentSessionId || 'default';
        const res = await fetch(`/v1/chat/context_telemetry?model=${encodeURIComponent(model)}&session_id=${encodeURIComponent(sessionId)}&tools=${encodeURIComponent(JSON.stringify(activeTools))}`);
        if (res.ok) {
            const data = await res.json();
            if (data.context_telemetry && window.updateLiveContextBar) {
                window.updateLiveContextBar({ context_telemetry: data.context_telemetry });
            }
        }
    } catch (e) {
        console.warn('Failed to refresh context telemetry:', e);
    }
};

function formatK(n) {
    if (!n && n !== 0) return '0';
    if (n >= 1048576) {
        const m = n / 1048576;
        return m % 1 === 0 ? m + 'M' : m.toFixed(1) + 'M';
    }
    if (n >= 1024) {
        const k = n / 1024;
        return k % 1 === 0 ? k + 'k' : (n >= 10000 ? Math.round(k) + 'k' : k.toFixed(1) + 'k');
    }
    return n.toString();
}

function renderCategoryTree(containerId, items, categoryType) {
    const container = document.getElementById(containerId);
    if (!container) return;
    container.innerHTML = '';

    // Strictly filter items: ONLY display tools that are actively loaded into context (taking tokens or in selectedSkills)
    const displayItems = (items || []).filter(item => {
        const isSelected = typeof selectedSkills !== 'undefined' && (selectedSkills.has(item.name) || selectedSkills.has(item.id));
        return item.status === 'active' || (item.tokens && item.tokens > 0) || isSelected;
    });

    if (displayItems.length === 0) {
        container.innerHTML = `<div style="color: #71717a; padding: 3px 6px; font-style: italic; font-size: 0.68rem;">No active ${categoryType} loaded in context</div>`;
        return;
    }

    displayItems.forEach((item, idx) => {
        const isLast = idx === displayItems.length - 1;
        const prefix = isLast ? '└──' : '├──';
        
        const customSvg = window.componentIcons?.get(item.name)?.customSvg 
            || window.componentIcons?.get(item.id)?.customSvg 
            || item.icon_svg;

        const iconHtml = customSvg 
            ? `<span style="width: 13px; height: 13px; display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; color: #a1a1aa;">${customSvg.replace('<svg ', '<svg style="width:100%; height:100%; max-width:13px; max-height:13px;" ')}</span>`
            : `<span style="flex-shrink: 0;">${categoryType === 'skill' ? '📋' : (categoryType === 'plugin' ? '🔌' : '🌐')}</span>`;

        let tagInfo = '';
        if (categoryType === 'skill') {
            tagInfo = `[Active • ${item.tokens || 42} tok • Ready]`;
        } else if (categoryType === 'plugin') {
            const runType = item.security_mode === 'sandboxed' ? 'WASM' : 'Native';
            const cap = item.memory_cap_mb || 16;
            tagInfo = `[${runType} • ${cap}MB cap • Ready]`;
        } else {
            tagInfo = `[npx • stdio • Ready]`;
        }

        const el = document.createElement('div');
        el.className = 'ctx-tree-item';
        el.style.cssText = 'display: flex; align-items: center; justify-content: space-between; padding: 2px 5px; border-radius: 4px; cursor: pointer; transition: background 0.15s; font-size: 0.70rem;';
        el.addEventListener('mouseenter', () => el.style.background = 'rgba(255,255,255,0.06)');
        el.addEventListener('mouseleave', () => el.style.background = 'transparent');

        el.innerHTML = `
            <span style="display: flex; align-items: center; gap: 5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                <span style="color: #71717a;">${prefix}</span> ${iconHtml} <span style="color: #f4f4f5; font-weight: 500;">${item.name}</span>
            </span>
            <span style="color: #a1a1aa; font-size: 0.64rem; margin-left: 6px; white-space: nowrap;">${tagInfo}</span>
        `;

        el.addEventListener('click', (e) => {
            e.stopPropagation();
            openDrilldownCard(item, categoryType);
        });

        container.appendChild(el);
    });
}

function openDrilldownCard(item, categoryType) {
    const mainView = document.getElementById('ctx-main-inspector-view');
    const card = document.getElementById('ctx-drilldown-card');
    if (!mainView || !card) return;

    mainView.style.display = 'none';
    card.style.display = 'block';

    const titleEl = document.getElementById('drilldown-title');
    const statusEl = document.getElementById('drilldown-status');
    const tokensEl = document.getElementById('drilldown-tokens');
    const latencyEl = document.getElementById('drilldown-latency');
    const memoryEl = document.getElementById('drilldown-memory');
    const fuelEl = document.getElementById('drilldown-fuel');
    const inputEl = document.getElementById('drilldown-input');
    const outputEl = document.getElementById('drilldown-output');

    const customSvg = window.componentIcons?.get(item.name)?.customSvg 
        || window.componentIcons?.get(item.id)?.customSvg 
        || item.icon_svg;

    const sec = item.security_mode === 'sandboxed' ? 'Sandboxed' : (item.security_mode === 'strict' ? 'Strict Sandbox' : 'Host PC');
    if (titleEl) {
        if (customSvg) {
            titleEl.innerHTML = `<span style="display: flex; align-items: center; gap: 5px;"><span style="width: 14px; height: 14px; display: inline-flex; align-items: center; justify-content: center; color: #60a5fa;">${customSvg.replace('<svg ', '<svg style="width:100%; height:100%;" ')}</span><span>${item.name}</span> <span style="color: #71717a; font-size: 0.65rem; font-weight: 400;">(${sec})</span></span>`;
        } else {
            const icon = categoryType === 'skill' ? '📋' : (categoryType === 'plugin' ? '🔌' : '🌐');
            titleEl.textContent = `${icon} ${item.name} (${sec})`;
        }
    }

    // Check if tool was executed in live session
    const executedInfo = window.latestExecutedTools?.get(item.name) || window.latestExecutedTools?.get(item.name?.toLowerCase());

    const isExecuted = !!executedInfo;
    if (statusEl) {
        statusEl.textContent = isExecuted ? 'EXECUTED (Completed in turn)' : 'LOADED (Active in context)';
        statusEl.style.color = isExecuted ? '#4ade80' : '#38bdf8';
    }

    if (tokensEl) {
        const tok = item.tokens || 42;
        tokensEl.textContent = isExecuted 
            ? `+${tok} tokens (Active • Evaluated)`
            : `+${tok} tokens (Allocated in prompt)`;
        tokensEl.style.color = '#facc15';
    }

    if (latencyEl) {
        if (isExecuted && executedInfo.latency_ms) {
            latencyEl.textContent = `${executedInfo.latency_ms.toFixed(2)} ms`;
            latencyEl.style.color = '#38bdf8';
        } else {
            latencyEl.textContent = '— (Awaiting invocation)';
            latencyEl.style.color = '#71717a';
        }
    }

    if (memoryEl) {
        const cap = item.memory_cap_mb || 16;
        if (isExecuted && executedInfo.memory_used_mb) {
            memoryEl.textContent = `${cap} MB (Used: ${executedInfo.memory_used_mb.toFixed(1)} MB)`;
        } else {
            memoryEl.textContent = `Limit: ${cap} MB (Standby)`;
        }
    }

    if (fuelEl) {
        if (isExecuted && executedInfo.cpu_fuel_consumed) {
            fuelEl.textContent = `${executedInfo.cpu_fuel_consumed.toLocaleString()} instructions`;
        } else {
            fuelEl.textContent = 'Sandboxed (Zero CPU fuel consumed)';
        }
    }

    if (inputEl) {
        if (isExecuted && executedInfo.input_payload) {
            inputEl.textContent = JSON.stringify(executedInfo.input_payload, null, 2);
        } else {
            inputEl.textContent = 'None (Tool not invoked in current turn)';
        }
    }

    if (outputEl) {
        if (isExecuted && executedInfo.output_result) {
            outputEl.textContent = JSON.stringify(executedInfo.output_result, null, 2);
        } else {
            outputEl.textContent = 'None (Tool not invoked in current turn)';
        }
    }
}

window.updateLiveContextBar = function(usage) {
    if (!usage) return;
    const breakdown = usage.context_telemetry?.context_breakdown || usage.context_breakdown;

    // 1. Generation Performance Stats (Left side)
    const tps = typeof usage.tokens_per_second === 'number' ? usage.tokens_per_second.toFixed(2) : (usage.tokens_per_second || '0.00');
    const ttft = typeof usage.time_to_first_token_ms === 'number' ? (usage.time_to_first_token_ms / 1000).toFixed(2) : '0.00';
    const time = typeof usage.total_time_ms === 'number' 
        ? (usage.total_time_ms / 1000).toFixed(2) 
        : (typeof usage.duration_ms === 'number' 
            ? (usage.duration_ms / 1000).toFixed(2) 
            : (typeof usage.total_duration_sec === 'number' 
                ? usage.total_duration_sec.toFixed(2) 
                : (usage.time_seconds ? Number(usage.time_seconds).toFixed(2) : '0.00')));
    const tokens = usage.total_tokens || usage.completion_tokens || 0;

    // Auto-Hide on bottom bar when generation completes
    const livePerfStats = document.getElementById('live-perf-stats');
    if (livePerfStats) livePerfStats.style.display = 'none';
    const tpsTag = document.getElementById('live-tps-tag');
    const timeTag = document.getElementById('live-time-tag');
    const ttftTag = document.getElementById('live-ttft-tag');
    const tokensTag = document.getElementById('live-tokens-tag');
    if (tpsTag) tpsTag.style.display = 'none';
    if (timeTag) timeTag.style.display = 'none';
    if (ttftTag) ttftTag.style.display = 'none';
    if (tokensTag) tokensTag.style.display = 'none';

    // 2. Context Window Bar (Right side)
    const usedEl = document.getElementById('live-ctx-used');
    const limitEl = document.getElementById('live-ctx-limit');
    const pctEl = document.getElementById('live-ctx-pct');

    if (breakdown) {
        const usableLimit = breakdown.total_context_limit;
        const modelMax = breakdown.model_native_context;
        const totalActive = breakdown.total_active_tokens || tokens;
        const activePct = breakdown.active_percentage || (usableLimit > 0 ? Math.round((totalActive / usableLimit) * 100) : 0);

        if (usedEl) usedEl.textContent = formatK(totalActive);
        if (limitEl) limitEl.textContent = formatK(usableLimit);
        if (pctEl) pctEl.textContent = `${activePct}%`;

        // 3. Popover Elements
        const popoverTotal = document.getElementById('popover-ctx-total');
        if (popoverTotal) popoverTotal.textContent = `${formatK(totalActive)} / ${formatK(usableLimit)} (${activePct}%)`;

        const popoverModelMax = document.getElementById('popover-model-max');
        if (popoverModelMax) popoverModelMax.textContent = formatK(modelMax);

        // Progress bar segments (5 pillars)
        const setBar = (id, pct) => {
            const el = document.getElementById(id);
            if (el) el.style.width = `${pct || 0}%`;
        };
        setBar('bar-messages', breakdown.messages_percentage);
        setBar('bar-system-prompt', breakdown.system_prompt_percentage);
        setBar('bar-skills', breakdown.skills_percentage);
        setBar('bar-plugins', breakdown.plugins_percentage || breakdown.system_tools_percentage);
        setBar('bar-mcp-tools', breakdown.mcp_tools_percentage);

        // List item values & percentages
        const setItem = (valId, pctId, val, pct) => {
            const vEl = document.getElementById(valId);
            const pEl = document.getElementById(pctId);
            if (vEl) vEl.textContent = formatK(val);
            if (pEl) pEl.textContent = `${pct || 0}%`;
        };
        setItem('item-messages-val', 'item-messages-pct', breakdown.messages_tokens, breakdown.messages_percentage);
        setItem('item-system-prompt-val', 'item-system-prompt-pct', breakdown.system_prompt_tokens, breakdown.system_prompt_percentage);
        setItem('item-skills-val', 'item-skills-pct', breakdown.skills_tokens, breakdown.skills_percentage);
        setItem('item-plugins-val', 'item-plugins-pct', breakdown.plugins_tokens || breakdown.system_tools_tokens, breakdown.plugins_percentage || breakdown.system_tools_percentage);
        setItem('item-mcp-tools-val', 'item-mcp-tools-pct', breakdown.mcp_tools_tokens, breakdown.mcp_tools_percentage);
        setItem('item-free-space-val', 'item-free-space-pct', breakdown.free_space_tokens, breakdown.free_space_percentage);

        // Update category count badges strictly from active in-context items
        const skillsCountEl = document.getElementById('skills-count-badge');
        const pluginsCountEl = document.getElementById('plugins-count-badge');
        const mcpCountEl = document.getElementById('mcp-count-badge');

        const activeSkills = (breakdown.skills?.items || []).filter(i => i.status === 'active' || (i.tokens && i.tokens > 0) || (typeof selectedSkills !== 'undefined' && (selectedSkills.has(i.name) || selectedSkills.has(i.id))));
        const activePlugins = (breakdown.plugins?.items || []).filter(i => i.status === 'active' || (i.tokens && i.tokens > 0));
        const activeMcp = (breakdown.mcp_tools?.items || []).filter(i => i.status === 'active' || (i.tokens && i.tokens > 0));

        if (skillsCountEl) skillsCountEl.textContent = `${activeSkills.length}`;
        if (pluginsCountEl) pluginsCountEl.textContent = `${activePlugins.length}`;
        if (mcpCountEl) mcpCountEl.textContent = `${activeMcp.length}`;

        // Populate tree structures strictly with active in-context items
        renderCategoryTree('skills-tree-list', activeSkills, 'skill');
        renderCategoryTree('plugins-tree-list', activePlugins, 'plugin');
        renderCategoryTree('mcp-tree-list', activeMcp, 'mcp');

    } else if (usage.total_tokens) {
        if (usedEl) usedEl.textContent = formatK(usage.total_tokens);
    }

    // 4. Wire Tree Toggles & Drill-Down Back Button (bound once)
    const setupTreeToggle = (btnId, listId) => {
        const btn = document.getElementById(btnId);
        const list = document.getElementById(listId);
        if (btn && list && !btn.dataset.bound) {
            btn.dataset.bound = 'true';
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                const isOpen = list.style.display === 'flex';
                if (isOpen) {
                    list.style.display = 'none';
                    btn.textContent = '▾ Open';
                } else {
                    list.style.display = 'flex';
                    btn.textContent = '▴ Close';
                }
            });
        }
    };
    setupTreeToggle('skills-tree-toggle', 'skills-tree-list');
    setupTreeToggle('plugins-tree-toggle', 'plugins-tree-list');
    setupTreeToggle('mcp-tree-toggle', 'mcp-tree-list');

    const backBtn = document.getElementById('drilldown-back-btn');
    if (backBtn && !backBtn.dataset.bound) {
        backBtn.dataset.bound = 'true';
        backBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            const mainView = document.getElementById('ctx-main-inspector-view');
            const card = document.getElementById('ctx-drilldown-card');
            if (mainView && card) {
                card.style.display = 'none';
                mainView.style.display = 'block';
            }
        });
    }

    // 5. Bind interactive popover toggle/hover on #live-ctx-wrapper
    const ctxWrapper = document.getElementById('live-ctx-wrapper');
    const popover = document.getElementById('live-context-popover');
    if (ctxWrapper && popover && !ctxWrapper.dataset.bound) {
        ctxWrapper.dataset.bound = 'true';
        // Open on hover
        ctxWrapper.addEventListener('mouseenter', () => {
            popover.classList.remove('hidden');
        });
        // Toggle on click
        ctxWrapper.addEventListener('click', (e) => {
            e.stopPropagation();
            popover.classList.toggle('hidden');
        });
        // Prevent clicks inside popover from closing it
        popover.addEventListener('click', (e) => {
            e.stopPropagation();
        });
        // Close ONLY when clicking strictly outside
        document.addEventListener('click', (e) => {
            if (!ctxWrapper.contains(e.target) && !popover.contains(e.target)) {
                popover.classList.add('hidden');
            }
        });
    }
};
