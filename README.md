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
```

Available algorithms: `backtracking`, `prim`, `kruskal`, `aldous_broder`,
`wilson`, `eller`. All of them produce a perfect maze, meaning exactly one path
connects any two cells.

## Maze file format

A saved maze is a plain text document: a `maze 1` marker line, a `width height`
line, and the ASCII rendering of the maze. The rendering is lossless, so loading
a saved maze reproduces the maze it came from.
