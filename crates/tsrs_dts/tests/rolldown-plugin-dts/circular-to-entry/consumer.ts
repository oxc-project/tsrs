import Manager from '__ENTRY__';
const manager = new Manager();
const item: (typeof manager.foos)[number] = { manager };
const same: Manager = item.manager;
manager.foos.push(item);
// @ts-expect-error recursive manager identity remains structured
manager.foos.push({ manager: 1 });
