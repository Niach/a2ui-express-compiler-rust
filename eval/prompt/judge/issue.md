# Issue detail (held out)

Request: An issue detail view: the title 'Login fails on Safari' with a status badge 'In progress' and a priority, tabs for 'Details' and 'Activity'; Details shows the description text and assignee avatar, Activity lists three timeline events; a 'Close issue' button in the header.

## Sample A

```
Stack
  Stack
    Stack
      Heading text="Login fails on Safari"
      Stack
        Badge text="In progress" variant="secondary" icon="status-in-progress"
        Badge text="High" variant="outline" icon="priority-high"
    Button label="Close issue" variant="default"
  Tabs value="details" tabs[2]
    Stack
      Text text="Users on Safari 17 are redirected back to the login page after submitting valid credentials. The session cookie does not appear to be set." variant="body"
      Stack
        Text text="Assignee" variant="muted"
        Avatar name="Dana Lee"
    Stack
      Row title="Issue created" icon="event-status-changed"
      Row title="Assigned to Dana Lee" icon="event-assignee-changed"
      Row title="Status changed to In progress" icon="event-status-changed"
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
