# Maze Generator

Just for fun. This tool can generate the random maze by different algorythms.
Each maze has only one path from left top corner to the right bottom corner.

## Usage

```sh
# Print a maze to the terminal
maze --command generate --width 20 --height 10 --algorithm backtracking

# Save a maze to a file
maze --command generate --width 20 --height 10 --algorithm eller --output maze.txt

# Verify a saved maze
maze --command check --input maze.txt

# Print a maze with its solution path drawn on it
maze --command solve --input maze.txt

# Reproduce a maze you liked
maze --command generate --width 20 --height 10 --algorithm wilson --seed 42
```

Available algorithms: `backtracking`, `prim`, `kruskal`, `aldous_broder`,
`wilson`, `eller`. All of them produce a perfect maze, meaning exactly one path
connects any two cells.

## Solving a maze

Because a perfect maze has exactly one route between two cells, `solve` prints
that route rather than a choice of routes. The entrance is marked `S`, the exit
`E`, and the corridors in between with `.`. The number of steps is reported
afterwards. A maze whose exit has been sealed off is printed unsolved and the
command exits with a failure status.

## Reproducible mazes

Every run picks a random seed and prints it, so passing `--seed` is optional.
The same seed with the same width, height and algorithm always produces the
same maze, which makes a maze worth keeping easy to regenerate.

## Maze file format

A saved maze is a plain text document: a `maze 1` marker line, a `width height`
line, and the ASCII rendering of the maze. The rendering is lossless, so loading
a saved maze reproduces the maze it came from.
