package org.mtrx.app.model

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import java.io.IOException
import kotlinx.coroutines.flow.first

/** Push registration's state across launches. */
val Context.pushPreferences: DataStore<Preferences> by preferencesDataStore(name = "push")

/** [PushStore] in [preferences]: keys `enabled`, `endpoint`, `permission_asked`. */
class DataStorePush(private val preferences: DataStore<Preferences>) : PushStore {
    override suspend fun enabled(): Boolean = read()?.get(ENABLED) ?: true

    override suspend fun setEnabled(value: Boolean) = write { it[ENABLED] = value }

    override suspend fun endpoint(): String? = read()?.get(ENDPOINT)

    override suspend fun setEndpoint(value: String?) = write {
        if (value == null) {
            it.remove(ENDPOINT)
        } else {
            it[ENDPOINT] = value
        }
    }

    override suspend fun permissionAsked(): Boolean = read()?.get(PERMISSION_ASKED) ?: false

    override suspend fun setPermissionAsked() = write { it[PERMISSION_ASKED] = true }

    private suspend fun read(): Preferences? = try {
        preferences.data.first()
    } catch (e: IOException) {
        null
    }

    private suspend fun write(change: (MutablePreferences) -> Unit) {
        try {
            preferences.edit { change(it) }
        } catch (e: IOException) {
            // Not remembered; registering again at the next start sets it.
        }
    }

    private companion object {
        val ENABLED = booleanPreferencesKey("enabled")
        val ENDPOINT = stringPreferencesKey("endpoint")
        val PERMISSION_ASKED = booleanPreferencesKey("permission_asked")
    }
}
