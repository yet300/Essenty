package dev.essenty.androidx.internal;

import androidx.activity.BackEventCompat;
import androidx.activity.OnBackPressedCallback;
import androidx.activity.OnBackPressedDispatcher;
import androidx.activity.OnBackPressedDispatcherOwner;
import androidx.lifecycle.DefaultLifecycleObserver;
import androidx.lifecycle.Lifecycle;
import androidx.lifecycle.LifecycleOwner;
import androidx.lifecycle.ViewModel;
import androidx.lifecycle.ViewModelStore;
import androidx.lifecycle.ViewModelStoreOwner;
import androidx.savedstate.SavedStateRegistry;
import androidx.savedstate.SavedStateRegistryOwner;

/**
 * Compile-time check of the AndroidX surface required by the private JNI bridge.
 *
 * <p>This class is deliberately not a working Rust callback adapter. The AAR
 * validates packaging and version compatibility only; runtime bridges require
 * a separate native-handle and main-thread implementation.
 */
final class AndroidXAvailability {
    private AndroidXAvailability() {}

    static void requireTypes(
            Lifecycle lifecycle,
            LifecycleOwner lifecycleOwner,
            DefaultLifecycleObserver lifecycleObserver,
            ViewModel viewModel,
            ViewModelStore viewModelStore,
            ViewModelStoreOwner viewModelStoreOwner,
            SavedStateRegistry savedStateRegistry,
            SavedStateRegistryOwner savedStateRegistryOwner,
            OnBackPressedDispatcher dispatcher,
            OnBackPressedDispatcherOwner dispatcherOwner,
            OnBackPressedCallback callback,
            BackEventCompat event) {
        // The signature forces javac to resolve every AndroidX API needed
        // for upstream parity. Gradle project dependencies carry them onward.
    }
}
