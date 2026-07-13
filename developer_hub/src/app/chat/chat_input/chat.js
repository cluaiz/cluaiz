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
    let wrapThreshold = Number.MAX_SAFE_INTEGER;
    const selectedSkills = new Set();

    // Dynamically fetch and populate models from backend
    fetchAndPopulateModels(modelMenu, selectedModelText, modelSelectBtn);

    // Textarea Auto-expand & Layout shift
    textarea.addEventListener('input', () => {
        // Handle Send Button visibility (Smooth transition)
        if (textarea.value.trim().length > 0 || isGenerating) {
            sendWrapper.classList.remove('w-0', 'opacity-0', 'scale-0');
            sendWrapper.classList.add('w-[2.25rem]', 'opacity-100', 'scale-100');
        } else {
            sendWrapper.classList.add('w-0', 'opacity-0', 'scale-0');
            sendWrapper.classList.remove('w-[2.25rem]', 'opacity-100', 'scale-100');
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
        if (sendBtn) {
            sendBtn.innerHTML = `
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="6" y="6" width="12" height="12"></rect>
                </svg>
            `;
            sendBtn.classList.add('text-red-500');
        }
        textarea.dispatchEvent(new Event('input'));
    });

    window.addEventListener('chat:complete', () => {
        isGenerating = false;
        if (sendBtn) {
            sendBtn.innerHTML = `
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <line x1="22" y1="2" x2="11" y2="13"></line>
                    <polygon points="22 2 15 22 11 13 2 9 22 2"></polygon>
                </svg>
            `;
            sendBtn.classList.remove('text-red-500');
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
            if (isGenerating) {
                window.dispatchEvent(new CustomEvent('chat:abort'));
            } else {
                sendMessage();
            }
        });
    }

    function sendMessage() {
        if (isGenerating) return;
        const content = textarea.value.trim();
        // If content is empty but we have an assistant message at the end of history, we might be 'continuing'
        // For simplicity, we just send empty message to trigger continue in chat_stream.js
        if (content.length > 0 || window.canContinue) {
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
            document.getElementById('skills-menu').classList.add('hidden');
            document.getElementById('thinking-menu').classList.add('hidden');

            if (menuId) {
                document.getElementById(menuId).classList.remove('hidden');
            }
        }, 150);
    }

    document.getElementById('skills-menu-wrapper').addEventListener('mouseenter', () => openSubmenu('skills-menu'));
    document.getElementById('thinking-menu-wrapper').addEventListener('mouseenter', () => openSubmenu('thinking-menu'));
    document.getElementById('upload-file-btn').addEventListener('mouseenter', () => openSubmenu(null));
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

            let iconStr = 'layers';
            if (skill === 'Web Search') iconStr = 'globe';
            else if (skill === 'Deep Research') iconStr = 'telescope';
            else if (skill === 'Think Deep') iconStr = 'brain';
            else if (skill === 'Think Lite') iconStr = 'zap';
            else if (skill === 'Long Answer') iconStr = 'align-justify';
            else if (skill === 'Short Answer') iconStr = 'align-left';

            chip.innerHTML = `
                <div class="icon-default flex-center">
                    <i data-lucide="${iconStr}" class="w-3-5 h-3-5"></i>
                </div>
                <div class="icon-hover flex-center" style="display: none;">
                    <i data-lucide="x" class="w-3-5 h-3-5 text-red-500"></i>
                </div>
                <span class="hidden sm:inline">${skill}</span>
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
        const response = await fetch('http://localhost:8000/v1/models/installed');
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
            const permRes = await fetch('http://localhost:8000/v1/system/permission');
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
                    const permRes = await fetch('http://localhost:8000/v1/system/permission');
                    if (permRes.ok) {
                        const permData = await permRes.json();
                        if (permData.permission) {
                            const newPerm = permData.permission;
                            if (!newPerm.chat_models) newPerm.chat_models = {};
                            newPerm.chat_models.text = model.id;
                            
                            await fetch('http://localhost:8000/v1/system/permission', {
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
