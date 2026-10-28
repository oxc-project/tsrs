// Read the existing benchmark recipes and the root pnpm installation for the Rust task.
import { catalog, overlaySha, projectPath, selectProjects } from "../../bench/lib.mjs";

import { publishedReference } from "../../tools/lib/typescript-reference.mjs";

const [mode, work, selection] = process.argv.slice(2);
if (mode === "inputs") {
    const cfg = await catalog();
    const inputs = await Promise.all(selectProjects(cfg, selection).map(async (project) => ({
        name: project.name,
        ...projectPath(project, work),
        identity: {
            project,
            suite: project.suite_dir ? cfg.suite : null,
            overlay: await overlaySha(project),
        },
    })));
    process.stdout.write(JSON.stringify(inputs));
} else if (mode === "reference") {
    process.stdout.write(JSON.stringify(publishedReference()));
} else {
    throw new Error("Expected inputs or reference");
}
