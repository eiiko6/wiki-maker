# Wiki Maker

## How to use

### Creation

> [!NOTE] See the example in [example/](./example/).

wiki-maker provides a CLI to help create a wiki.

You can generate an entry with:
```sh
wiki-maker entry new "My New Entry"
```

An entry is made of essencially 2 files:
- `my-new-entry.toml` contains the names of the other files, as well as a map of attributes for your entry
- `my-new-entry.md` is where you start writing
- By default it looks for `my-new-entry.png` as the main image.

Links are made between entries by using this syntax: `[See this entry](my-other-entry)`.  
You can view which links are not yet satisfied with:
```sh
wiki-maker todo
```

You can even generate a graph representation of your wiki:
```sh
wiki-maker graph | dot -Tpng > /tmp/graph.png
imv /tmp/graph.png
```

### Deployment

wiki-maker provides 2 ways to deploy your wiki.  

You can serve the wiki in `example` with:
```sh
wiki-maker --path ./example/ serve --port 8090 --host
```

> [!NOTE] Changes to files will instantly reflect on the server.

Or you can generate a static build with:
```sh
wiki-maker --path ./example/ build -o ./dist/
```
