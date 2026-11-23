# Notification inbox

Request: A notifications inbox: a header 'Notifications' with a 'Mark all as read' action, and a list of five notifications, each with the sender's avatar, a one-line message, a relative time, and an unread marker on the first two.

## Sample A

```
Page title="Notifications"
  [slot trailing]
    Button label="Mark all as read" variant="outline" on=[press→event markAllRead]
  List divided=true children=[template item, once per item of $/notifications]
[template item: repeated once per item of its list]
  Row title=$message meta=$time
    [slot leading]
      Avatar name=$sender seed=$id
    [slot trailing]
      Badge text="New" variant="default"
[data model] /notifications = [{"id":"n1","sender":"Maya Chen","message":"Maya Chen commented on your pull request","time":"2m ago","unread":true},{"id":"n2","sender":"Jonas Weber","message":"Jonas Weber mentioned you in Design review","time":"15m ago","unread":true},{"id":"n3","sender":"Priya Nair","message":"Priya Nair assigned you to EXP-142","time":"1h ago","unread":false},{"id":"n4","sender":"Luca Romano","message":"Luca Romano approved your changes","time":"3h ago","unread":false},{"id":"n5","sender":"Sofia Alvarez","message":"Sofia Alvarez shared a file with you","time":"Yesterday","unread":false}]
```

## Sample B

```
Page title="Notifications"
  [slot trailing]
    Button label="Mark all as read" variant="outline" on=[press→event mark_all_read]
  Card
    List divided=true children=[template item, once per item of $/notifications]
[template item: repeated once per item of its list]
  Row title=$message subtitle=$sender meta=$time
    [slot leading]
      Avatar name=$sender
    [slot trailing]
      Badge text="New"
[data model] {"notifications":[{"sender":"Maya Chen","message":"Maya Chen commented on your pull request","time":"2m ago","unread":true},{"sender":"Luis Ortega","message":"Luis Ortega assigned you to Billing refactor","time":"18m ago","unread":true},{"sender":"Priya Nair","message":"Priya Nair mentioned you in #design-review","time":"1h ago","unread":false},{"sender":"Tom Becker","message":"Tom Becker approved your deploy request","time":"3h ago","unread":false},{"sender":"Ana Souza","message":"Ana Souza shared the Q3 roadmap with you","time":"Yesterday","unread":false}]}
```
