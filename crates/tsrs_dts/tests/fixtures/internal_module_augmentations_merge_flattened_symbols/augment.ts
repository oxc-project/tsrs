import './dep.js'; interface Private { value: string } declare module './dep.js' { interface Item { extra: Private } }
