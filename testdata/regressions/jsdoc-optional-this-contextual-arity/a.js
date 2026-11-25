/** @type {{ m: (x: number) => number }} */
const obj = {
  /**
   * @this {any}
   * @param {any} [this]
   */
  m(...args) { args[1]; return args.length; }
};
