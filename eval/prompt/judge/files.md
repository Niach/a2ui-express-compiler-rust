# File browser

Request: A file browser: a breadcrumb path (Home / Projects / Website), a search field, and a table of six files with name, size, modified date and a row action menu (Rename, Delete).

## Sample A

```
Page title="Files"
  Stack
    Breadcrumb items[3]
    Input label="Search files" placeholder="Search files" value={"@path":"/query"} name="query" type="search"
    Table columns[4]
      [slot actions]
        Menu items[2]
          Button variant="ghost" icon="ellipsis"
[data model] {"query":"","files":[{"id":"f1","name":"index.html","size":"4.2 KB","modified":"2026-03-10T09:15:00Z"},{"id":"f2","name":"styles.css","size":"12.8 KB","modified":"2026-03-11T14:02:00Z"},{"id":"f3","name":"app.js","size":"86 KB","modified":"2026-03-12T16:40:00Z"},{"id":"f4","name":"logo.png","size":"23 KB","modified":"2026-03-05T11:30:00Z"},{"id":"f5","name":"README.md","size":"1.1 KB","modified":"2026-03-01T08:00:00Z"},{"id":"f6","name":"sitemap.xml","size":"2.4 KB","modified":"2026-03-13T18:25:00Z"}]}
```

## Sample B

```
Page title="Files"
  Stack
    Breadcrumb items[3]
    Toolbar
      Input label="Search" placeholder="Search files" value={"@path":"/query"} name="query" type="search"
    Table columns[4]
      [slot actions]
        Menu items[2]
          Button label="Actions" variant="ghost" icon="ellipsis"
[data model] / = {"query":"","files":[{"id":"1","name":"index.html","size":"4.2 KB","modified":"2025-03-02"},{"id":"2","name":"styles.css","size":"12.8 KB","modified":"2025-03-01"},{"id":"3","name":"app.js","size":"86 KB","modified":"2025-02-27"},{"id":"4","name":"logo.svg","size":"3.1 KB","modified":"2025-02-20"},{"id":"5","name":"hero.jpg","size":"1.4 MB","modified":"2025-02-18"},{"id":"6","name":"README.md","size":"1.9 KB","modified":"2025-02-10"}]}
```
