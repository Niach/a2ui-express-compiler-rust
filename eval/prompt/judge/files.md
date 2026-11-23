# File browser

Request: A file browser: a breadcrumb path (Home / Projects / Website), a search field, and a table of six files with name, size, modified date and a row action menu (Rename, Delete).

## Sample A

```
Page title="Files"
  Stack direction="vertical" gap="md"
    Breadcrumb items[3: "Home" | "Projects" | "Website"] on=[select→event navigate]
    Card title="Website"
      Stack direction="vertical" gap="md"
        Input label="Search files" name="query" type="search" placeholder="Search by name" value=$/query on=[change→event search {query: $/query}]
        Table columns[4: "Name" | "Size" | "Modified" | ""] rows=$/files rowKey="name" caption="Files in Website" emptyText="No files found"
          [slot actions]
            Menu icon="ellipsis" items[2: "Rename" | "Delete"] on=[select→event file_action {name: $name}]
              Button icon="ellipsis" variant="ghost" size="icon"
[data model] {"query":"","files":[{"name":"index.html","size":"4.2 KB","modified":"2026-03-14T09:12:00Z"},{"name":"styles.css","size":"18.7 KB","modified":"2026-03-12T16:40:00Z"},{"name":"app.js","size":"92 KB","modified":"2026-03-13T11:05:00Z"},{"name":"logo.svg","size":"2.1 KB","modified":"2026-02-28T08:30:00Z"},{"name":"hero.png","size":"1.4 MB","modified":"2026-03-01T14:22:00Z"},{"name":"README.md","size":"1.8 KB","modified":"2026-02-20T10:00:00Z"}]}
```

## Sample B

```
Page title="Files"
  Stack gap="md"
    Breadcrumb items[3: "Home" | "Projects" | "Website"]
    Input label="Search files" name="query" type="search" placeholder="Search files" value=$/query
    Table caption="Website" rowKey="id" columns[4: "Name" | "Size" | "Modified" | ""] rows=filter(items=$/files, query=$/query, fields=["name"])
      [slot actions]
        Menu label="Row actions" items[2: "Rename" | "Delete"]
          Button variant="ghost" size="icon" icon="ellipsis" label="Actions"
[data model] / = {"query":"","files":[{"id":"f1","name":"index.html","size":"4.2 KB","modified":"2025-03-02"},{"id":"f2","name":"styles.css","size":"12.8 KB","modified":"2025-03-01"},{"id":"f3","name":"app.js","size":"86 KB","modified":"2025-02-27"},{"id":"f4","name":"logo.svg","size":"2.1 KB","modified":"2025-02-20"},{"id":"f5","name":"hero.png","size":"1.4 MB","modified":"2025-02-18"},{"id":"f6","name":"README.md","size":"1.1 KB","modified":"2025-02-10"}]}
```
