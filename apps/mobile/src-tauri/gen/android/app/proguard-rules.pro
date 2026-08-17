# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
# L'installateur de mises à jour appelle `FileProvider.getUriForFile` depuis le
# code natif, en JNI. R8 ne voit pas cet appel — aucun code Java ou Kotlin ne
# passe par là — et supprime la méthode : au moment d'installer, la machine
# virtuelle répond `NoSuchMethodError`. La classe est conservée par le
# manifeste ; c'est la méthode qu'il faut nommer.
-keep class androidx.core.content.FileProvider {
    public static android.net.Uri getUriForFile(android.content.Context, java.lang.String, java.io.File);
}
