# Music player

Request: A music player: the current track's cover art, title and artist, a progress bar, play/pause, previous and next controls, and a queue of the next four tracks below.

## Sample A

```
Page title="Now playing"
  Stack gap="lg"
    Card
      Stack gap="md" align="center"
        Image src=$/track/cover alt=$/track/title aspectRatio=1 width=240 fallback="music" style{borderRadius}
        Stack gap="xs" align="center"
          Heading text=$/track/title level="h2"
          Text text=$/track/artist variant="muted"
        Progress value=$/progress max=100 valueText=concat(values=[$/elapsed, " / ", $/track/duration]) style{width}
        Stack direction="horizontal" gap="md" justify="center"
          Button icon="skip-back" label="Previous" variant="outline" size="icon" on=[press→event previous]
          Button icon=cond(if=$/playing, then="pause", else="play") label=cond(if=$/playing, then="Pause", else="Play") size="icon" on=[press→[{functionCall: set(path="/playing", value=not(value=$/playing))}, {event: {name: "togglePlay", context: {playing: $/playing}}}]]
          Button icon="skip-forward" label="Next" variant="outline" size="icon" on=[press→event next]
    Section title="Up next" count=len(value=$/queue)
      List divided=true children=[template queueRow, once per item of $/queue]
[template queueRow: repeated once per item of its list]
  Row title=$title subtitle=$artist meta=$duration icon="music" pressable=true on=[press→event playTrack {id: $id}]
[data model] / = {"playing":true,"progress":42,"elapsed":"1:34","track":{"title":"Midnight City","artist":"M83","cover":"https://example.com/covers/midnight-city.jpg","duration":"3:44"},"queue":[{"id":"q1","title":"Genesis","artist":"Justice","duration":"4:03"},{"id":"q2","title":"Instant Crush","artist":"Daft Punk","duration":"5:37"},{"id":"q3","title":"Intro","artist":"The xx","duration":"2:07"},{"id":"q4","title":"Electric Feel","artist":"MGMT","duration":"3:49"}]}
```

## Sample B

```
Page title="Music player"
  Stack gap="lg"
    Card title="Now playing"
      Stack gap="md"
        Image src=$/track/cover alt="Album cover" fit="cover" aspectRatio=1 fallback="music"
        Heading text=$/track/title level="h2"
        Text text=$/track/artist variant="muted"
        Progress value=$/progress max=100 label="Progress" valueText=$/track/elapsed detail=$/track/total
        Stack direction="horizontal" gap="md" justify="center" align="center"
          Button icon="media-skip-previous" variant="outline" size="icon" on=[press→event previous]
          Button icon=cond(if=$/playing, then="pause", else="play") size="icon" on=[press→event toggle_play]
          Button icon="media-skip-next" variant="outline" size="icon" on=[press→event next]
    Card title="Up next"
      List divided=true children=[template item, once per item of $/queue]
[template item: repeated once per item of its list]
  Row title=$title subtitle=$artist meta=$duration icon="music"
[data model] {"track":{"title":"Midnight Drive","artist":"Neon Harbor","cover":"https://example.com/covers/midnight-drive.jpg","elapsed":"1:24","total":"3:42"},"progress":38,"playing":true,"queue":[{"title":"Glass Ocean","artist":"Luma Vale","duration":"3:15"},{"title":"Paper Moons","artist":"The Quiet Hours","duration":"4:02"},{"title":"Static Bloom","artist":"Neon Harbor","duration":"2:58"},{"title":"Afterglow","artist":"Sunday Static","duration":"3:47"}]}
```
