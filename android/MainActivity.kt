package dev.dioxus.main

// The dx-generated Kotlin helpers (Logger.kt) reference BuildConfig from the app namespace.
typealias BuildConfig = com.h3d.app.BuildConfig

// Dioxus Native renders through winit + wgpu, so the app runs as a plain NativeActivity
// (the default dx template targets the webview renderer's WryActivity).
class MainActivity : android.app.NativeActivity()
