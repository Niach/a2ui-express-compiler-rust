# Music player

Request: A music player: the current track's cover art, title and artist, a progress bar, play/pause, previous and next controls, and a queue of the next four tracks below.

## Sample A

```
(nothing rendered: the reply did not parse)
```

## Sample B

```
Page title="Now playing"
  Stack
    Image
    Stack
      Heading text={"@path":"/track/title"}
      Text text={"@path":"/track/artist"} variant="muted"
    Progress value={"@path":"/player/progress"}
    Stack
      Button label="Previous" variant="ghost" icon="skip-back"
      Button label={"@path":"/player/playLabel"} variant="default" icon={"@path":"/player/icon"}
      Button label="Next" variant="ghost" icon="skip-forward"
    Section title="Up next"
      List
[template queueRow: repeated once per item of its list]
  Row title={"@path":"title"} subtitle={"@path":"artist"} icon="music"
[data model] / = {"track":{"title":"Midnight Drive","artist":"The Neon Hearts","cover":"https://example.com/covers/midnight-drive.jpg"},"player":{"progress":35,"time":"1:24 / 3:58","icon":"pause","playLabel":"Pause"},"queueCount":4,"queue":[{"id":"t2","title":"City Lights","artist":"Luna Park","duration":"3:12"},{"id":"t3","title":"Golden Hour","artist":"Sam Rivers","duration":"4:05"},{"id":"t4","title":"Paper Planes","artist":"The Neon Hearts","duration":"2:47"},{"id":"t5","title":"Slow Tide","artist":"Aria Vale","duration":"3:36"}]}
```
