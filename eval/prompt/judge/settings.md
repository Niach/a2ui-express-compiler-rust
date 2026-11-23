# Settings page

Request: A notification settings page: a section 'Email' with switches for 'Product updates' and 'Weekly digest', a section 'Appearance' with a theme select (Light, Dark, System) and a font-size slider (12 to 20), and a Save / Cancel row at the bottom.

## Sample A

```
Page title="Notification settings"
  Form name="notificationSettings"
    Section title="Email"
      Switch name="productUpdates" label="Product updates" checked=$/settings/productUpdates
      Switch name="weeklyDigest" label="Weekly digest" checked=$/settings/weeklyDigest
    Section title="Appearance"
      Select name="theme" label="Theme" options[3: "Light" | "Dark" | "System"] value=$/settings/theme
      Slider label="Font size" min=12 max=20 step=1 value=$/settings/fontSize
    FormActions submit="Save" cancel=true on=[cancel→event cancel]
[data model] / = {"settings":{"productUpdates":true,"weeklyDigest":false,"theme":"system","fontSize":16}}
```

## Sample B

```
Page title="Notification settings"
  Form name="notification_settings" on=[submit→event save_settings {productUpdates: $/settings/productUpdates, weeklyDigest: $/settings/weeklyDigest, theme: $/settings/theme, fontSize: $/settings/fontSize}]
    Card title="Email"
      Switch label="Product updates" name="productUpdates" checked=$/settings/productUpdates description="News about new features and improvements" on=[change→event update_setting {key: "productUpdates", value: $/settings/productUpdates}]
      Switch label="Weekly digest" name="weeklyDigest" checked=$/settings/weeklyDigest description="A summary of your activity every week" on=[change→event update_setting {key: "weeklyDigest", value: $/settings/weeklyDigest}]
    Card title="Appearance"
      Select label="Theme" name="theme" options[3: "Light" | "Dark" | "System"] value=$/settings/theme
      Slider label="Font size" min=12 max=20 step=1 value=$/settings/fontSize showValue=true
    FormActions submit="Save" cancel=true on=[cancel→event cancel_settings]
[data model] {"settings":{"productUpdates":true,"weeklyDigest":false,"theme":"system","fontSize":16}}
```
