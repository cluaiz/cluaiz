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
        // For simplicity, we just send empty message to trigger continue in chat_stream.js
        if (content.length > 0 || window.canContinue) {
            isStopped = false;
            window.dispatchEvent(new CustomEvent('chat:send', { detail: { message: content } }));
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
            const allMenus = ['skills-menu', 'plugins-menu', 'extensions-menu', 'mcp-menu', 'thinking-menu'];
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
    document.getElementById('extensions-menu-wrapper')?.addEventListener('mouseenter', () => openSubmenu('extensions-menu'));
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
        if (isAttachOpen && !attachWrapper.contains(e.target)) {
            isAttachOpen = false;
            attachMenu.classList.add('hidden');
            attachMenu.classList.remove('flex');
            const icon = attachBtn.firstElementChild;
            if (icon) icon.classList.remove('rotate-45');
            attachBtn.classList.remove('text-accent', 'bg-secondary');
            attachBtn.classList.add('text-muted');
        }
        if (isModelOpen && !modelWrapper.contains(e.target)) {
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

            chip.innerHTML = `
                <div class="icon-default flex-center">
                    ${defaultIconHtml}
                </div>
                <div class="icon-hover flex-center" style="display: none;">
                    <i data-lucide="x" class="w-3-5 h-3-5 text-red-500"></i>
                </div>
                <span class="hidden sm:inline">${formattedName}</span>
            `;

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
                renderSkills();
            });

            container.appendChild(chip);
        });
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

    // Think Toggle
    const thinkToggle = document.getElementById('think-toggle');
    const thinkToggleThumb = document.getElementById('think-toggle-thumb');
    const thinkOptionsOn = document.getElementById('think-options-on');
    const thinkOptionsOff = document.getElementById('think-options-off');
    thinkToggle.addEventListener('click', (e) => {
        e.stopPropagation();
        isThinkModeOn = !isThinkModeOn;
        if (isThinkModeOn) {
            thinkToggle.classList.replace('bg-secondary', 'bg-accent');
            thinkToggle.style.borderColor = 'transparent';
            thinkToggleThumb.style.transform = 'translateY(-50%) translateX(14px)';
            thinkOptionsOn.style.display = 'flex';
            thinkOptionsOff.style.display = 'none';
        } else {
            thinkToggle.classList.replace('bg-accent', 'bg-secondary');
            thinkToggle.style.borderColor = 'var(--border-color)';
            thinkToggleThumb.style.transform = 'translateY(-50%) translateX(2px)';
            thinkOptionsOn.style.display = 'none';
            thinkOptionsOff.style.display = 'flex';
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
        const installed = data.installed || [];

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

        // Fetch active model from Permission.json
        let activeModelId = chatModels.length > 0 ? chatModels[0].id : 'default';
        try {
            const permRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
            if (permRes.ok) {
                const permData = await permRes.json();
                if (permData.permission?.chat_models?.text) {
                    // Strict Rule: Always use what is in Permission.json, do not fallback to random array index
                    activeModelId = permData.permission.chat_models.text;
                }
            }
        } catch (e) {
            console.error('Failed to read active model:', e);
        }

        // Set the active model
        const activeFormatted = formatModelName(activeModelId);
        selectedModelText.textContent = activeFormatted.shortName;

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

                // Update active model in Permission.json via API
                try {
                    const permRes = await fetch(window.getApiBaseUrl() + '/v1/system/permission');
                    if (permRes.ok) {
                        const permData = await permRes.json();
                        if (permData.permission) {
                            const newPerm = permData.permission;
                            if (!newPerm.chat_models) newPerm.chat_models = {};
                            newPerm.chat_models.text = model.id;
                            
                            await fetch(window.getApiBaseUrl() + '/v1/system/permission', {
                                method: 'POST',
                                headers: { 'Content-Type': 'application/json' },
                                body: JSON.stringify(newPerm)
                            });
                        }
                    }
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
        
        const categories = [
            { key: 'skill', menuId: 'skills-menu', icon: 'layers', colorClass: 'hover-text-accent' },
            { key: 'plugin', menuId: 'plugins-menu', icon: 'box', colorClass: 'hover-text-blue' },
            { key: 'extension', menuId: 'extensions-menu', icon: 'puzzle', colorClass: 'hover-text-amber' },
            { key: 'mcp', menuId: 'mcp-menu', icon: 'server', colorClass: 'hover-text-emerald' }
        ];

        for (const cat of categories) {
            const menu = document.getElementById(cat.menuId);
            const allItems = data[cat.key] || [];
            
            if (menu) {
                menu.innerHTML = ''; // clear dummy data
                
                const enabledItems = [];
                for (const itemName of allItems) {
                    try {
                        const setRes = await fetch(`/api/components/settings?component_type=${cat.key}&component_id=${itemName}`);
                        const setData = await setRes.json();
                        if (setData.status === 'success' && setData.values && setData.values.enabled) {
                            enabledItems.push(itemName);
                        }
                    } catch (e) {
                        console.warn(`Could not fetch settings for ${itemName}`, e);
                    }
                }

                if (enabledItems.length === 0) {
                    const displayName = cat.key === 'mcp' ? 'MCPs' : cat.key.charAt(0).toUpperCase() + cat.key.slice(1) + 's';
                    menu.innerHTML = `<div class="p-2 text-xs text-muted text-center italic">No ${displayName} available</div>`;
                    continue;
                }

                enabledItems.forEach(async (itemName) => {
                    const btn = document.createElement('button');
                    btn.className = `dropdown-item w-full flex-between text-primary ${cat.colorClass} hover-bg-secondary group skill-btn`;
                    btn.setAttribute('data-skill', itemName);
                    
                    let iconHtml = `<i data-lucide="${cat.icon}" class="w-4 h-4 text-muted"></i>`;
                    let rawSvgStr = null;
                    try {
                        const svgRes = await fetch(`/api/components/file?component_type=${cat.key}&component_id=${itemName}&file_path=assets/icon.svg`);
                        const svgData = await svgRes.json();
                        if (svgData.status === 'success' && svgData.content) {
                            let svgStr = svgData.content.trim();
                            if (svgStr.startsWith('<svg')) {
                                rawSvgStr = svgStr;
                                iconHtml = `<div class="text-muted flex-center" style="width: 16px; height: 16px; display: inline-flex; align-items: center; justify-content: center;">
                                    ${svgStr.replace('<svg ', '<svg style="width:100%; height:100%;" ')}
                                </div>`;
                            }
                        }
                    } catch (e) {
                        console.warn(`Failed to fetch SVG for ${itemName}`, e);
                    }

                    window.componentIcons = window.componentIcons || new Map();
                    window.componentIcons.set(itemName, { catIcon: cat.icon, customSvg: rawSvgStr });

                    const formattedName = itemName.split('-').map(word => word.charAt(0).toUpperCase() + word.slice(1)).join(' ');

                    btn.innerHTML = `
                        <div class="flex-align gap-2-5">
                            ${iconHtml}
                            ${formattedName}
                        </div>
                    `;
                    
                    btn.addEventListener('click', (e) => {
                        e.stopPropagation(); // keep menu open or let it close?
                        // If it's a toggle:
                        if (selectedSkills.has(itemName)) {
                            selectedSkills.delete(itemName);
                        } else {
                            selectedSkills.add(itemName);
                        }
                        updateSkillMenuVisuals();
                        renderSkills();
                    });
                    
                    menu.appendChild(btn);
                    if (window.lucide) window.lucide.createIcons();
                });
            }
        }
        if (window.lucide) window.lucide.createIcons();
    } catch (e) {
        console.error("Failed to fetch tools/skills list:", e);
    }
}
