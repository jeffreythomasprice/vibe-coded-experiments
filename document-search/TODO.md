
in flight:

status messages are still garbage
e.g.
"extracting (2675s ago)"
should say something like
"Extracting group (level 0/3, group 55/77) (2675s ago)"
make sure the status messages are the same if we're waiting on the command to finish (i.e. not detached) and if we're looking at the status command output

ideally this output also includes a rough estimate of how far along we are in the process in terms of overal number of steps and estimated time remaining

logging should be at least as verbose: groups and index counts, estimated operations and time remaining
