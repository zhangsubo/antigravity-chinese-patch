export const VERSION = '3.0.1';
export const GLOBAL_KEY = '__antigravity_chinese_patch__';
export const LEGACY_KEY = '__teacherxu_antigravity_zh__';
export const SWARM_GLOBAL_KEY = '__antigravity_chinese_patch_swarm__';
export const ORIGINALS_KEY = '__antigravity_chinese_patch_originals__';
export const CUSTOM_DICT_KEY = '__agyZhCustomTranslations';
export const MODE_KEY = '__AGY_ZH_MODE__';

export const RUNTIME_MARKER = `/* Antigravity Chinese Patch Runtime v${VERSION} */`;
export const SWARM_MARKER = `/* Antigravity Chinese Patch Swarm Runtime v${VERSION} */`;

export const LANG_CHANNEL = '__antigravity_chinese_patch_lang__';
export const QUERY_LANG_CHANNEL = '__antigravity_chinese_patch_query_lang__';

export const STORAGE_LANG = 'agyZhLang';
export const STORAGE_POS = 'agyZhBallPos';
export const STORAGE_KEEP_EN = 'agyZhKeepEnglishLabels';
export const CONTROL_ATTR = 'data-agy-zh-control';

export const ENGLISH_OPTION_LABELS = new Set([
  'this conversation',
  'request review',
  'default',
  'auto',
  'turbo',
  'high',
  'medium',
  'low',
]);
