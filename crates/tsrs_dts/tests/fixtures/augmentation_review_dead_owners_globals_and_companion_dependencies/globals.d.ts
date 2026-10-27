interface GlobalItem { global: true } declare module 'virtual' { interface Item { local: import('./dep.js').Item } }
