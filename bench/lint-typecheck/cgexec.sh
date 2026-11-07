#!/bin/bash
# cgexec.sh <cgroup dir> <argv...>: join the cgroup, drop from root to uid 1000, then exec argv.
cg=$1
shift
echo $$ > "$cg/cgroup.procs"
exec setpriv --reuid=1000 --regid=1000 --init-groups -- "$@"
