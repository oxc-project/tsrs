const path = require('node:path');
const binding = require('./binding.cjs');

function argumentsFor(options = {}) {
  if (options === null || typeof options !== 'object' || Array.isArray(options)) {
    throw new TypeError('options must be an object');
  }
  for (const name of ['cwd', 'project']) {
    if (options[name] !== undefined && (typeof options[name] !== 'string' || options[name].length === 0 || options[name].includes('\0'))) {
      throw new TypeError(`${name} must be a nonempty path string without NUL bytes`);
    }
  }
  const cwd = path.resolve(options.cwd ?? process.cwd());
  return [cwd, path.resolve(cwd, options.project ?? '.')];
}

exports.check = function check(options) {
  return binding.checkNative(...argumentsFor(options));
};

exports.checkSync = function checkSync(options) {
  return binding.checkSyncNative(...argumentsFor(options));
};
