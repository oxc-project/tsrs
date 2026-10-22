import { parentPort, workerData } from 'node:worker_threads';
import { checkSync } from '../src-js/index.mjs';
parentPort.postMessage(checkSync(workerData));
