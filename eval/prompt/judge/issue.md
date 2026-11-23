# Issue detail

Request: An issue detail view: the title 'Login fails on Safari' with a status badge 'In progress' and a priority, tabs for 'Details' and 'Activity'; Details shows the description text and assignee avatar, Activity lists three timeline events; a 'Close issue' button in the header.

## Sample A

```
Page title=$/issue/title
  [slot trailing]
    Button label="Close issue" icon="circle-check" variant="default" on=[press→event close_issue]
  Stack gap="md"
    Stack direction="horizontal" gap="sm"
      Badge text="In progress" variant="default" icon="loader"
      Badge text="High priority" variant="destructive" icon="flag"
    Tabs tabs[2: "Details" | "Activity"] value="details"
      Stack gap="md"
        Text text=$/issue/description
        Stack direction="horizontal" gap="sm" align="center"
          Avatar name=$/issue/assignee
          Text text=$/issue/assignee variant="label"
      Section title="Activity" children=[template eventRow, once per item of $/events]
[template eventRow: repeated once per item of its list]
  Row title=$title subtitle=$subtitle meta=$meta icon=$icon
[data model] / = {"issue":{"title":"Login fails on Safari","description":"Users on Safari 17 are redirected back to the login page after submitting valid credentials. The session cookie is not being stored, which looks related to the SameSite attribute set on the auth response.","assignee":"Maya Chen"},"events":[{"id":"e1","title":"Status changed to In progress","subtitle":"Maya Chen","meta":"2h ago","icon":"circle-dot"},{"id":"e2","title":"Assigned to Maya Chen","subtitle":"Alex Rivera","meta":"5h ago","icon":"user"},{"id":"e3","title":"Issue created","subtitle":"Alex Rivera","meta":"1d ago","icon":"plus"}]}
```

## Sample B

```
Page title="Login fails on Safari"
  [slot trailing]
    Button label="Close issue" variant="outline" on=[press→event close_issue]
  Stack direction="horizontal" gap="sm" align="center"
    Badge text="In progress" variant="secondary" icon="status-in-progress"
    Badge text="High priority" variant="outline" icon="priority-high"
  Tabs tabs[2: "Details" | "Activity"] value="details"
    Card title="Details"
      Markdown text=$/issue/description
      Row title=$/issue/assignee subtitle="Assignee" icon="user"
        [slot leading]
          Avatar name=$/issue/assignee
    Card title="Activity"
      List divided=true children=[template eventRow, once per item of $/events]
[template eventRow: repeated once per item of its list]
  Row title=$title subtitle=$when icon=$icon
[data model] {"issue":{"description":"Users on Safari 17 cannot sign in: the login button stays disabled after the password is entered. Chrome and Firefox are not affected.","assignee":"Dana Reyes"},"events":[{"title":"Issue created","when":"2 days ago","icon":"ui-add"},{"title":"Status changed to In progress","when":"Yesterday","icon":"event-status-changed"},{"title":"Assigned to Dana Reyes","when":"Today","icon":"event-assignee-changed"}]}
```
