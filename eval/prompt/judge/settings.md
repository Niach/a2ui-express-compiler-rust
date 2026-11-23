# Settings page

Request: A notification settings page: a section 'Email' with switches for 'Product updates' and 'Weekly digest', a section 'Appearance' with a theme select (Light, Dark, System) and a font-size slider (12 to 20), and a Save / Cancel row at the bottom.

## Sample A

```
Page title="Notification settings"
  Form name="notifications"
    Section title="Email"
      Switch label="Product updates" name="productUpdates"
      Switch label="Weekly digest" name="weeklyDigest"
    Section title="Appearance"
      Select label="Theme" value={"@path":"/form/theme"} name="theme" options[3]
      Slider label="Font size" value={"@path":"/form/fontSize"}
    FormActions
[data model] {"form":{"productUpdates":true,"weeklyDigest":false,"theme":"system","fontSize":16}}
```

## Sample B

```
Page title="Notification settings"
  Form name="notificationSettings"
    Section title="Email"
      Switch label="Product updates" name="productUpdates"
      Switch label="Weekly digest" name="weeklyDigest"
    Section title="Appearance"
      Select label="Theme" value={"@path":"/settings/theme"} name="theme" options[3]
      Slider label="Font size" value={"@path":"/settings/fontSize"}
    FormActions
[data model] / = {"settings":{"productUpdates":true,"weeklyDigest":false,"theme":"system","fontSize":14}}
```
