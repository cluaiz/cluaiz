export class Sidebar {
    /**
     * @param {Object} props
     * @param {Array} props.apiData - The JSON array of API groups and endpoints
     * @param {Function} props.onSelect - Callback when an endpoint is clicked
     * @param {string} [props.className] - Extra CSS classes
     */
    constructor(props) {
        this.apiData = props.apiData || [];
        this.onSelect = props.onSelect || (() => {});
        this.className = props.className || '';
        
        this.element = null;
        this.activeLink = null;
    }

    render() {
        this.element = document.createElement('div');
        this.element.className = `nav-container ${this.className}`;

        this.apiData.forEach((group, groupIdx) => {
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
                if (!isOpen) {
                    titleEl.classList.add('open');
                } else {
                    titleEl.classList.remove('open');
                }
            };

            group.endpoints.forEach((ep) => {
                const link = document.createElement('div');
                link.className = 'nav-link';
                // Attach endpoint data for easy matching later
                link.dataset.method = ep.method;
                link.dataset.path = ep.path;
                
                const methodClass = 'method-' + ep.method.toLowerCase();
                link.innerHTML = `<span class="method-badge ${methodClass}">${ep.method}</span> <span style="white-space:nowrap; overflow:hidden; text-overflow:ellipsis;">${ep.path}</span>`;

                link.onclick = () => {
                    if (this.activeLink) {
                        this.activeLink.classList.remove('active');
                    }
                    link.classList.add('active');
                    this.activeLink = link;
                    
                    this.onSelect(ep);
                };

                itemsEl.appendChild(link);
            });

            if (groupIdx === 0) {
                itemsEl.classList.add('open');
                titleEl.classList.add('open');
            }

            groupEl.appendChild(titleEl);
            groupEl.appendChild(itemsEl);
            this.element.appendChild(groupEl);
        });

        return this.element;
    }
    
    // Programmatically select an endpoint from outside
    selectEndpoint(path, method) {
        if (!this.element) return;
        const norm = p => '/' + (p || '').trim().replace(/^\/+/, '').replace(/\{[^}]+\}/g, '<param>').replace(/<[^>]+>/g, '<param>');
        const targetNorm = norm(path);
        const links = this.element.querySelectorAll('.nav-link');
        links.forEach(l => {
            l.classList.remove('active');
            if (norm(l.dataset.path) === targetNorm && (!method || l.dataset.method.toUpperCase() === method.toUpperCase())) {
                l.classList.add('active');
                this.activeLink = l;
                // Auto-expand parent nav-items and title
                const parentItems = l.closest('.nav-items');
                if (parentItems) {
                    parentItems.classList.add('open');
                    const parentTitle = parentItems.previousElementSibling;
                    if (parentTitle) parentTitle.classList.add('open');
                }
            }
        });
    }
}
