import { normalizeWhitespace } from '../shared/translator.js';

export function createMenuTranslator(translator, isEnabled) {
  function translateMenu(items, parentId = '') {
    if (!isEnabled() || !Array.isArray(items)) return items;
    return items.map((item) => {
      if (!item || typeof item !== 'object') return item;
      const skipLabel = parentId === 'fork' || (parentId === 'move-to-group' && item.type === 'checkbox');
      const key = !skipLabel && typeof item.label === 'string' ? normalizeWhitespace(item.label) : '';
      const target = key ? translator.translate(key, { normalized: true }) : null;
      const label = target ? item.label.replace(/\S[\s\S]*\S|\S/, target) : item.label;
      const submenu = Array.isArray(item.submenu) ? translateMenu(item.submenu, item.id || '') : item.submenu;
      return label === item.label && submenu === item.submenu ? item : { ...item, label, submenu };
    });
  }

  return translateMenu;
}
