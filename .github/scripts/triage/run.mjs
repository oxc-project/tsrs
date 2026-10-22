import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// A run step uses underscore names; upstream accepts these as legacy input names.
// Keep write/provider credentials in the action process, outside agent sandboxes.
Object.assign(process.env, {
    INPUT_READ_TOKEN: process.env.GH_READ_TOKEN ?? '',
    INPUT_WRITE_TOKEN: process.env.GH_WRITE_TOKEN ?? '',
    INPUT_CLOUDFLARE_API_KEY: process.env.CLOUDFLARE_API_KEY ?? '',
    INPUT_CLOUDFLARE_ACCOUNT_ID: process.env.CLOUDFLARE_ACCOUNT_ID ?? '',
    INPUT_TRIAGE_MODEL: 'cloudflare-workers-ai/@cf/moonshotai/kimi-k2.7-code',
    INPUT_VERIFICATION_MODEL: 'cloudflare-workers-ai/@cf/moonshotai/kimi-k2.6',
    INPUT_TRIAGE_SKILL: '.agents/skills/triage',
    INPUT_PR_SKILL: '.agents/skills/pr-writer',
    INPUT_AUTO_PR_ON_FIX: 'true',
    INPUT_BOT_LOGINS: 'oxc-guard[bot]',
});
await import(pathToFileURL(resolve('target/triagebot-action/dist/index.mjs')).href);
