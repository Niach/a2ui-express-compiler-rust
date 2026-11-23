# Notification inbox

Request: A notifications inbox: a header 'Notifications' with a 'Mark all as read' action, and a list of five notifications, each with the sender's avatar, a one-line message, a relative time, and an unread marker on the first two.

## Sample A

```
Page title="Notifications"
  Button label="Mark all as read" variant="outline"
  List
[template item: repeated once per item of its list]
  Row title={"@path":"message"}
    [slot leading]
      Avatar name={"@path":"sender"}
    [slot trailing]
      Badge text="New" variant="default"
[data model] /notifications = [{"id":"n1","sender":"Ada Lovelace","message":"Ada commented on your pull request","time":"2026-11-23T09:55:00Z","unread":true},{"id":"n2","sender":"Grace Hopper","message":"Grace assigned you to EXP-42","time":"2026-11-23T09:20:00Z","unread":true},{"id":"n3","sender":"Alan Turing","message":"Alan merged the release branch","time":"2026-11-23T07:30:00Z","unread":false},{"id":"n4","sender":"Linus Torvalds","message":"Linus mentioned you in a review","time":"2026-11-22T16:00:00Z","unread":false},{"id":"n5","sender":"Margaret Hamilton","message":"Margaret shared a document with you","time":"2026-11-21T11:00:00Z","unread":false}]
```

## Sample B

```
Stack
  Stack
    Heading text="Notifications"
    Button label="Mark all as read" variant="outline"
  List
[template item: repeated once per item of its list]
  Row title={"@path":"message"}
    [slot leading]
      Avatar name={"@path":"sender"}
    [slot trailing]
      Badge text="New"
[data model] {"items":[{"sender":"Ava Chen","message":"Ava Chen commented on your pull request","time":"2 min ago","unread":true},{"sender":"Marcus Lee","message":"Marcus Lee mentioned you in Q3 planning","time":"18 min ago","unread":true},{"sender":"Priya Nair","message":"Priya Nair approved your design review","time":"1 hour ago","unread":false},{"sender":"Tom Becker","message":"Tom Becker assigned you an issue","time":"3 hours ago","unread":false},{"sender":"Sofia Rossi","message":"Sofia Rossi shared a file with you","time":"Yesterday","unread":false}]}
```
