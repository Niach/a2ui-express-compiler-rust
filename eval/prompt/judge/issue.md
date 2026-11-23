# Issue detail

Request: An issue detail view: the title 'Login fails on Safari' with a status badge 'In progress' and a priority, tabs for 'Details' and 'Activity'; Details shows the description text and assignee avatar, Activity lists three timeline events; a 'Close issue' button in the header.

## Sample A

```
Page title={"@path":"/issue/title"}
  [slot trailing]
    Button label="Close issue"
  Stack
    Badge text={"@path":"/issue/status"} variant="secondary"
    Badge text={"@path":"/issue/priority"} variant="outline" icon="priority-high"
  Tabs value="details" tabs[2]
    Stack
      Markdown text={"@path":"/issue/description"}
      Row title={"@path":"/issue/assignee"} subtitle="Assignee"
        [slot leading]
          Avatar name={"@path":"/issue/assignee"}
    List
[template eventRow: repeated once per item of its list]
  Row title={"@path":"title"} subtitle={"@path":"detail"} icon="event-status-changed"
[data model] {"issue":{"title":"Login fails on Safari","status":"In progress","priority":"High","description":"Users on Safari 17 are redirected back to the login page after submitting valid credentials. The session cookie is not being stored.","assignee":"Dana Reyes"},"events":[{"title":"Issue created","detail":"Reported by Sam Lee","time":"2 days ago"},{"title":"Assigned to Dana Reyes","detail":"Status set to In progress","time":"1 day ago"},{"title":"Comment added","detail":"Reproduced on Safari 17.2","time":"3 hours ago"}]}
```

## Sample B

```
Page title={"@path":"/issue/title"}
  [slot trailing]
    Button label="Close issue" variant="default"
  Stack
    Badge text={"@path":"/issue/status"} variant="secondary"
    Badge text={"@path":"/issue/priority"} variant="outline" icon="flag"
  Tabs value={"@path":"/tab"} tabs[2]
    Stack
      Text text={"@path":"/issue/description"}
      Stack
        Avatar name={"@path":"/issue/assignee"}
        Text text={"@path":"/issue/assignee"} variant="label"
    List
[template eventRow: repeated once per item of its list]
  Row title={"@path":"title"} icon="circle-dot"
[data model] / = {"issue":{"title":"Login fails on Safari","status":"In progress","priority":"High","description":"Users on Safari 17 are unable to log in: the form submits but the session cookie is never set, so they are redirected back to the login page.","assignee":"Maria Keller"},"events":[{"id":"e1","title":"Issue created","meta":"2 days ago"},{"id":"e2","title":"Assigned to Maria Keller","meta":"Yesterday"},{"id":"e3","title":"Status changed to In progress","meta":"2 hours ago"}],"tab":"details"}
```
