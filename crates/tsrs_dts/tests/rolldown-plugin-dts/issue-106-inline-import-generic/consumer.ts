import { CalendarDataManager } from '__ENTRY__';
const manager = new CalendarDataManager();
manager.emitter = {};
const emitter: {} = manager.emitter;
// @ts-expect-error mapped optional properties over an empty interface still exclude null
manager.emitter = null;
// @ts-expect-error the class remains a zero-argument constructor
new CalendarDataManager(1);
